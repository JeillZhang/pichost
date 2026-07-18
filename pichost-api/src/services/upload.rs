use std::sync::Arc;

use axum::extract::Multipart;
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::cache::CachePool;
use crate::middleware::auth::AuthUser;
use crate::services::html_escape;
use deadpool_redis::redis::AsyncCommands;

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResult {
    pub id: Uuid,
    pub public_key: String,
    pub original_name: String,
    pub url: String,
    pub markdown: String,
    pub html: String,
    pub bbcode: String,
    pub sha256: String,
    pub file_size: i64,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub status: String,
    pub thumbnail_url: Option<String>,
    pub webp_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for GET /api/v1/images
#[derive(Debug, Deserialize)]
pub struct ImageListQuery {
    /// Page number (1-based, default 1)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (default 20, max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    /// Sort field: "created_at", "file_size", "original_name"
    #[serde(default = "default_sort")]
    pub sort: String,
    /// Sort order: "asc" or "desc"
    #[serde(default = "default_order")]
    pub order: String,
    /// Optional search term (ILIKE match against original_name)
    #[serde(default)]
    pub search: String,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }
fn default_sort() -> String { "created_at".to_string() }
fn default_order() -> String { "desc".to_string() }

/// Paginated response envelope
#[derive(Debug, Serialize)]
pub struct ImageListResponse {
    pub items: Vec<UploadResult>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

async fn enqueue_processing_task(
    redis_pool: &CachePool,
    image_id: Uuid,
    user_id: Uuid,
    storage_key: &str,
    mime_type: &str,
) {
    let task_id = Uuid::new_v4();
    let payload = serde_json::json!({
        "task_id": task_id.to_string(),
        "image_id": image_id.to_string(),
        "user_id": user_id.to_string(),
        "storage_backend": "local",
        "source_key": storage_key,
        "source_mime": mime_type,
        "retry_count": 0,
        "max_retries": 3,
    });

    let pool_err = |e: deadpool_redis::PoolError| {
        tracing::warn!("redis pool error during enqueue: {e}");
    };

    let mut conn = match redis_pool.get().await {
        Ok(c) => c,
        Err(e) => {
            pool_err(e);
            return;
        }
    };

    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    let now = chrono::Utc::now().to_rfc3339();
    let task_key = format!("pichost:task:{task_id}");

    // Store task data hash — field names must match queue.rs convention
    let _: Result<(), _> = conn.hset(&task_key, "data", &payload_json).await;
    let _: Result<(), _> = conn.hset(&task_key, "status", "pending").await;
    let _: Result<(), _> = conn.hset(&task_key, "created_at", &now).await;
    let _: Result<(), _> = conn.hset(&task_key, "updated_at", &now).await;
    // Push to pending queue
    let _: Result<(), _> = conn
        .lpush("pichost:tasks:pending", task_id.to_string())
        .await;

    tracing::info!(%task_id, %image_id, "enqueued processing task");
}

pub async fn process_upload(
    state: Arc<AppState>,
    user: AuthUser,
    mut multipart: Multipart,
) -> Result<UploadResult, (StatusCode, Json<serde_json::Value>)> {
    // ---- Extract file from multipart ----
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut _content_type: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            file_name = field.file_name().map(|s| s.to_string());
            _content_type = field.content_type().map(|s| s.to_string());
            let data = field.bytes().await.map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("failed to read file: {e}")})),
                )
            })?;
            file_bytes = Some(data.to_vec());
            break;
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "no file field found in upload"})),
        )
    })?;

    // ---- Validate it's an image ----
    if !infer::is_image(&bytes) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "file is not a valid image"})),
        ));
    }

    // ---- Check file size ----
    let max_size = if user.is_admin {
        state.config.upload.max_file_size_admin
    } else {
        state.config.upload.max_file_size_user
    };
    if bytes.len() as u64 > max_size {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "file exceeds maximum allowed size"})),
        ));
    }

    if let Some(quota) = user.storage_quota {
        let current_usage: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(file_size), 0) FROM images WHERE user_id = $1",
        )
        .bind(user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Quota usage query failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;

        let new_file_size = bytes.len() as i64;
        if current_usage + new_file_size > quota {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(serde_json::json!({
                    "error": "storage quota exceeded",
                    "quota_bytes": quota,
                    "used_bytes": current_usage,
                    "file_bytes": new_file_size,
                })),
            ));
        }
    }

    // ---- Compute SHA256 ----
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&bytes);
    let sha256 = format!("{:x}", hash);

    // ---- Dedup check ----
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM images WHERE user_id=$1 AND sha256=$2)")
            .bind(user.id)
            .bind(&sha256)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| {
                tracing::warn!("Dedup query failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                )
            })?;

    if exists {
        let row = sqlx::query_as::<_, (Uuid, String, String, String, String, i64, String, String)>(
            r#"SELECT id, public_key, original_name, storage_key, mime_type, file_size, url, sha256
               FROM images WHERE user_id = $1 AND sha256 = $2"#,
        )
        .bind(user.id)
        .bind(&sha256)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to fetch existing image: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;

        let (image_id, public_key, original_name, _storage_key, _mime_type, file_size, url, sha256) =
            row;

        let markdown = format!("![{}]({})", original_name, url);
        let html = format!(
            "<img src=\"{}\" alt=\"{}\" />",
            url,
            html_escape(&original_name)
        );
        let bbcode = format!("[img]{}[/img]", url);

        return Ok(UploadResult {
            id: image_id,
            public_key,
            original_name,
            url,
            markdown,
            html,
            bbcode,
            sha256,
            file_size,
            mime_type: _mime_type.clone(),
            width: None,
            height: None,
            status: "active".to_string(),
            thumbnail_url: None,
            webp_url: None,
            created_at: chrono::Utc::now(),
        });
    }

    // ---- Generate unique public key ----
    use rand::Rng;
    let public_key = loop {
        let key = format!("{:06x}", rand::thread_rng().gen::<u32>() & 0xFFFFFF);
        let key_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM images WHERE public_key=$1)")
                .bind(&key)
                .fetch_one(&state.pool)
                .await
                .map_err(|e| {
                    tracing::warn!("Public key uniqueness check failed: {e}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "internal server error"})),
                    )
                })?;
        if !key_exists {
            break key;
        }
    };

    // ---- Storage key ----
    let storage_key = format!("{}/{}", user.id, public_key);

    // ---- Detect MIME type from bytes ----
    let mime_type = infer::get(&bytes)
        .map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // ---- Detect image dimensions ----
    let (width, height): (Option<i32>, Option<i32>) = {
        let cursor = std::io::Cursor::new(&bytes);
        match image::ImageReader::new(cursor).with_guessed_format() {
            Ok(reader) => match reader.into_dimensions() {
                Ok((w, h)) => (Some(w as i32), Some(h as i32)),
                Err(e) => {
                    tracing::warn!("Failed to decode image dimensions: {e}");
                    (None, None)
                }
            },
            Err(e) => {
                tracing::warn!("Failed to create image reader: {e}");
                (None, None)
            }
        }
    };

    // ---- Write to storage ----
    let storage = state.router.default_backend();
    storage
        .put(&storage_key, &bytes, &mime_type)
        .await
        .map_err(|e| {
            tracing::warn!("Storage write failed on {}: {e}", storage.backend_name());
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage write failed"})),
            )
        })?;

    // ---- Build URL and link formats ----
    let original_name = file_name.unwrap_or_else(|| "file".to_string());
    let url = if storage.backend_name() == "local" {
        format!(
            "{}/u/{}",
            state.config.server.public_url.trim_end_matches('/'),
            public_key
        )
    } else {
        storage.public_url(&storage_key)
    };
    let markdown = format!("![{}]({})", original_name, url);
    let html = format!(
        "<img src=\"{}\" alt=\"{}\" />",
        url,
        html_escape(&original_name)
    );
    let bbcode = format!("[img]{}[/img]", url);
    let file_size = bytes.len() as i64;

    // ---- INSERT into images table ----
    let image_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO images
           (user_id, public_key, original_name, storage_key, storage_backend,
            mime_type, file_size, width, height, sha256, url, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'active')
           RETURNING id"#,
    )
    .bind(user.id)
    .bind(&public_key)
    .bind(&original_name)
    .bind(&storage_key)
    .bind("local")
    .bind(&mime_type)
    .bind(file_size)
    .bind(width)
    .bind(height)
    .bind(&sha256)
    .bind(&url)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Image insert failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "failed to save image metadata"})),
        )
    })?;

    // ---- Enqueue async processing task ----
    enqueue_processing_task(
        &state.cache.get_pool(),
        image_id,
        user.id,
        &storage_key,
        &mime_type,
    )
    .await;

    Ok(UploadResult {
        id: image_id,
        public_key,
        original_name,
        url,
        markdown,
        html,
        bbcode,
        sha256,
        file_size,
        mime_type,
        width,
        height,
        status: "active".to_string(),
        thumbnail_url: None,
        webp_url: None,
        created_at: chrono::Utc::now(),
    })
}
