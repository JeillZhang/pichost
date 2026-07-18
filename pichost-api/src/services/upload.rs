use std::sync::Arc;

use axum::extract::Multipart;
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use uuid::Uuid;

use crate::app::AppState;
use crate::cache::CachePool;
use crate::middleware::auth::AuthUser;
use crate::services::html_escape;
use deadpool_redis::redis::AsyncCommands;

// ── Types ──────────────────────────────────────────────────────────────────

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

/// Full image-row tuple used by list_images / get_image queries.
/// Fields: id, public_key, original_name, url, mime_type, file_size,
///         sha256, width, height, status, thumbnail_url, webp_url, created_at
pub(crate) type ImageRow = (
    Uuid,
    String,
    String,
    String,
    String,
    i64,
    String,
    Option<i32>,
    Option<i32>,
    String,
    Option<String>,
    Option<String>,
    DateTime<Utc>,
);

impl UploadResult {
    /// Build an UploadResult from a DB row tuple. Generates markdown, HTML and
    /// bbcode link formats from original_name + url.
    pub(crate) fn from_row(row: ImageRow) -> Self {
        let (id, public_key, original_name, url, mime_type, file_size, sha256, width, height, status, thumbnail_url, webp_url, created_at) =
            row;
        let markdown = format!("![{}]({})", original_name, url);
        let html = format!(
            "<img src=\"{}\" alt=\"{}\" />",
            url,
            html_escape(&original_name)
        );
        let bbcode = format!("[img]{}[/img]", url);
        Self {
            id,
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
            status,
            thumbnail_url,
            webp_url,
            created_at,
        }
    }
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

// ── Enqueue helper ─────────────────────────────────────────────────────────

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

// ── Private helpers ────────────────────────────────────────────────────────

type ApiError = (StatusCode, Json<serde_json::Value>);

/// Extracts the first `file` field from a multipart body.
/// Returns raw bytes + original filename (defaults to "file").
async fn extract_file_from_multipart(
    mut multipart: Multipart,
) -> Result<(Vec<u8>, String), ApiError> {
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            let file_name = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "file".to_string());
            let data = field.bytes().await.map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("failed to read file: {e}")})),
                )
            })?;
            return Ok((data.to_vec(), file_name));
        }
    }
    Err((
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": "no file field found in upload"})),
    ))
}

/// Validates file against max-size limits (admin vs user) and per-user
/// storage quota.  file_size is in bytes.
async fn check_upload_quotas(
    state: &AppState,
    user: &AuthUser,
    file_size: u64,
) -> Result<(), ApiError> {
    let max_size = if user.is_admin {
        state.config.upload.max_file_size_admin
    } else {
        state.config.upload.max_file_size_user
    };
    if file_size > max_size {
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

        let new_file_size = file_size as i64;
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
    Ok(())
}

/// Checks whether this user already uploaded the same content (sha256 dedup).
/// Returns `Some(UploadResult)` if a duplicate exists, `None` otherwise.
async fn try_dedup(
    state: &AppState,
    user: &AuthUser,
    sha256: &str,
) -> Result<Option<UploadResult>, ApiError> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM images WHERE user_id=$1 AND sha256=$2)")
            .bind(user.id)
            .bind(sha256)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| {
                tracing::warn!("Dedup query failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                )
            })?;

    if !exists {
        return Ok(None);
    }

    let row = sqlx::query_as::<_, ImageRow>(
        r#"SELECT id, public_key, original_name, url, mime_type, file_size,
                  sha256, width, height, status, thumbnail_url, webp_url, created_at
           FROM images WHERE user_id = $1 AND sha256 = $2"#,
    )
    .bind(user.id)
    .bind(sha256)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Failed to fetch existing image: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    Ok(Some(UploadResult::from_row(row)))
}

/// Generates a collision-free 6-char hex public key.
async fn generate_public_key(state: &AppState) -> Result<String, ApiError> {
    use rand::Rng;
    loop {
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
            return Ok(key);
        }
    }
}

/// Detect MIME type from raw bytes. Falls back to application/octet-stream.
fn detect_mime(bytes: &[u8]) -> String {
    infer::get(bytes)
        .map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

/// Detects image width × height from raw bytes.
/// Returns `(None, None)` on failure — callers degrade gracefully.
fn image_dimensions(bytes: &[u8]) -> (Option<i32>, Option<i32>) {
    let cursor = std::io::Cursor::new(bytes);
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
}

/// Writes bytes to storage, builds the public URL, and INSERTs into the DB.
/// Returns `(image_id, storage_key)`.
#[allow(clippy::too_many_arguments)]
async fn persist_image(
    state: &AppState,
    user: &AuthUser,
    public_key: &str,
    original_name: &str,
    bytes: &[u8],
    mime_type: &str,
    width: Option<i32>,
    height: Option<i32>,
    sha256: &str,
) -> Result<(Uuid, String), ApiError> {
    let storage_key = format!("{}/{}", user.id, public_key);

    let storage = state.router.default_backend();
    storage
        .put(&storage_key, bytes, mime_type)
        .await
        .map_err(|e| {
            tracing::warn!("Storage write failed on {}: {e}", storage.backend_name());
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage write failed"})),
            )
        })?;

    let url = if storage.backend_name() == "local" {
        format!(
            "{}/u/{}",
            state.config.server.public_url.trim_end_matches('/'),
            public_key
        )
    } else {
        storage.public_url(&storage_key)
    };

    let image_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO images
           (user_id, public_key, original_name, storage_key, storage_backend,
            mime_type, file_size, width, height, sha256, url, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'active')
           RETURNING id"#,
    )
    .bind(user.id)
    .bind(public_key)
    .bind(original_name)
    .bind(&storage_key)
    .bind("local")
    .bind(mime_type)
    .bind(bytes.len() as i64)
    .bind(width)
    .bind(height)
    .bind(sha256)
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

    Ok((image_id, storage_key))
}

/// Constructs a fully-populated UploadResult for a freshly-inserted image.
/// Generates all link formats (markdown, html, bbcode).
#[allow(clippy::too_many_arguments)]
fn build_result(
    state: &AppState,
    id: Uuid,
    pk: String,
    name: String,
    bytes: &[u8],
    mime: String,
    w: Option<i32>,
    h: Option<i32>,
    sha256: String,
) -> UploadResult {
    let url = format!(
        "{}/u/{}",
        state.config.server.public_url.trim_end_matches('/'),
        pk
    );
    let file_size = bytes.len() as i64;
    let markdown = format!("![{}]({})", name, url);
    let html = format!("<img src=\"{}\" alt=\"{}\" />", url, html_escape(&name));
    let bbcode = format!("[img]{}[/img]", url);
    UploadResult {
        id,
        public_key: pk,
        original_name: name,
        url,
        markdown,
        html,
        bbcode,
        sha256,
        file_size,
        mime_type: mime,
        width: w,
        height: h,
        status: "active".to_string(),
        thumbnail_url: None,
        webp_url: None,
        created_at: Utc::now(),
    }
}

// ── Public API ─────────────────────────────────────────────────────────────

pub async fn process_upload(
    state: Arc<AppState>,
    user: AuthUser,
    multipart: Multipart,
) -> Result<UploadResult, ApiError> {
    let (bytes, file_name) = extract_file_from_multipart(multipart).await?;

    if !infer::is_image(&bytes) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "file is not a valid image"})),
        ));
    }

    check_upload_quotas(&state, &user, bytes.len() as u64).await?;

    let sha256 = format!("{:x}", sha2::Sha256::digest(&bytes));

    if let Some(existing) = try_dedup(&state, &user, &sha256).await? {
        return Ok(existing);
    }

    let public_key = generate_public_key(&state).await?;
    let mime_type = detect_mime(&bytes);
    let (width, height) = image_dimensions(&bytes);

    let (image_id, storage_key) = persist_image(
        &state, &user, &public_key, &file_name, &bytes, &mime_type,
        width, height, &sha256,
    )
    .await?;

    enqueue_processing_task(
        &state.cache.get_pool(), image_id, user.id, &storage_key, &mime_type,
    )
    .await;

    Ok(build_result(
        &state, image_id, public_key, file_name, &bytes,
        mime_type, width, height, sha256,
    ))
}
