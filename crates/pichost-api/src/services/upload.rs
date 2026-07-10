use std::sync::Arc;

use axum::extract::Multipart;
use axum::http::StatusCode;
use axum::Json;
use pichost_core::storage::StorageBackend;
use serde::Serialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::middleware::auth::AuthUser;
use crate::services::html_escape;

#[derive(Debug, Serialize)]
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

    // ---- Compute SHA256 ----
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&bytes);
    let sha256 = format!("{:x}", hash);

    // ---- Dedup check ----
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM images WHERE user_id=$1 AND sha256=$2)",
    )
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
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "duplicate image"})),
        ));
    }

    // ---- Generate unique public key ----
    use rand::Rng;
    let public_key = loop {
        let key = format!("{:06x}", rand::thread_rng().gen::<u32>() & 0xFFFFFF);
        let key_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM images WHERE public_key=$1)",
        )
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

    // ---- Write to LocalStorage ----
    let storage = pichost_core::storage::local::LocalStorage::new(
        state.config.storage.local_base_path.clone(),
        state.config.server.public_url.clone(),
    );
    storage
        .put(&storage_key, &bytes, &mime_type)
        .await
        .map_err(|e| {
            tracing::warn!("Storage write failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage write failed"})),
            )
        })?;

    // ---- Build URL and link formats ----
    let original_name = file_name.unwrap_or_else(|| "file".to_string());
    let url = format!(
        "{}/u/{}",
        state.config.server.public_url.trim_end_matches('/'),
        public_key
    );
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
    .bind(None::<i32>) // width — skipped
    .bind(None::<i32>) // height — skipped
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
    })
}
