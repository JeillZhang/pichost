use std::sync::Arc;

use axum::{
    extract::{Extension, Multipart, Path, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use pichost_core::storage::StorageBackend;
use serde_json::json;
use uuid::Uuid;

use crate::app::AppState;
use crate::middleware::auth::AuthUser;
use crate::services::html_escape;
use crate::services::upload::{self, UploadResult};

/// POST /api/v1/images — upload an image (protected)
pub async fn upload_handler(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResult>), (StatusCode, Json<serde_json::Value>)> {
    let result = upload::process_upload(state, user, multipart).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// GET /api/v1/images — list user's images (protected)
pub async fn list_images(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<UploadResult>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = sqlx::query_as::<_, (
        Uuid, String, String, String, String, i64, String,
        Option<i32>, Option<i32>, String,
        Option<String>, Option<String>, chrono::DateTime<chrono::Utc>,
    )>(
        r#"SELECT id, public_key, original_name, url, mime_type, file_size,
                  sha256, width, height, status, thumbnail_url, webp_url, created_at
           FROM images WHERE user_id = $1 ORDER BY created_at DESC LIMIT 50"#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("List images query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal server error"})),
        )
    })?;

    let images = rows
        .into_iter()
        .map(|(id, public_key, original_name, url, mime_type, file_size,
              sha256, width, height, status, thumbnail_url, webp_url, created_at)| {
            UploadResult {
                id, public_key,
                original_name: original_name.clone(),
                url: url.clone(),
                markdown: format!("![{}]({})", original_name, url),
                html: format!(
                    "<img src=\"{}\" alt=\"{}\" />",
                    url,
                    html_escape(&original_name)
                ),
                bbcode: format!("[img]{}[/img]", url),
                sha256, file_size, mime_type, width, height, status,
                thumbnail_url, webp_url, created_at,
            }
        })
        .collect();

    Ok(Json(images))
}

/// GET /api/v1/images/{id} — get image metadata (protected)
pub async fn get_image(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<UploadResult>, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (
        Uuid, String, String, String, String, i64, String,
        Option<i32>, Option<i32>, String,
        Option<String>, Option<String>, chrono::DateTime<chrono::Utc>,
    )>(
        r#"SELECT id, public_key, original_name, url, mime_type, file_size,
                  sha256, width, height, status, thumbnail_url, webp_url, created_at
           FROM images WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Get image query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal server error"})),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "image not found"})),
        )
    })?;

    let (id, public_key, original_name, url, mime_type, file_size,
         sha256, width, height, status, thumbnail_url, webp_url, created_at) = row;

    Ok(Json(UploadResult {
        id, public_key,
        original_name: original_name.clone(),
        url: url.clone(),
        markdown: format!("![{}]({})", original_name, url),
        html: format!(
            "<img src=\"{}\" alt=\"{}\" />",
            url,
            html_escape(&original_name)
        ),
        bbcode: format!("[img]{}[/img]", url),
        sha256, file_size, mime_type, width, height, status,
        thumbnail_url, webp_url, created_at,
    }))
}

/// GET /u/{public_key} — serve image publicly (unauthenticated)
pub async fn public_get(
    State(state): State<Arc<AppState>>,
    Path(public_key): Path<String>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT storage_key, mime_type, status FROM images WHERE public_key = $1",
    )
    .bind(&public_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Public image query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal server error"})),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "image not found"})),
        )
    })?;

    let (storage_key, mime_type, status) = row;

    // Only serve active or ready images
    if status != "active" && status != "ready" {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "image not found"})),
        ));
    }

    // Read from LocalStorage
    let storage = pichost_core::storage::local::LocalStorage::new(
        state.config.storage.local_base_path.clone(),
        state.config.server.public_url.clone(),
    );
    let bytes = storage.get(&storage_key).await.map_err(|e| {
        tracing::warn!("Storage read failed: {e}");
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "image not found"})),
        )
    })?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &mime_type)
        .header(
            header::CACHE_CONTROL,
            "public, max-age=31536000, immutable",
        )
        .body(axum::body::Body::from(bytes))
        .unwrap();

    Ok(response)
}

fn mime_for_thumb_key(key: &str) -> &'static str {
    if key.ends_with(".png") { "image/png" }
    else { "image/jpeg" }
}

/// GET /u/thumb/{image_id} — serve generated thumbnail (unauthenticated)
pub async fn public_get_thumb(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<Uuid>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT thumbnail_key FROM images WHERE id = $1 AND status IN ('active', 'ready')",
    )
    .bind(image_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Thumb query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (thumb_key,) = row;
    let thumb_key = thumb_key.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({"error": "thumbnail not yet generated"})))
    })?;

    let storage = pichost_core::storage::local::LocalStorage::new(
        state.config.storage.local_base_path.clone(),
        state.config.server.public_url.clone(),
    );
    let bytes = storage.get(&thumb_key).await.map_err(|e| {
        tracing::warn!("Thumb storage read failed: {e}");
        (StatusCode::NOT_FOUND, Json(json!({"error": "thumbnail not found"})))
    })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_for_thumb_key(&thumb_key))
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

/// GET /u/webp/{image_id} — serve generated WebP (unauthenticated)
pub async fn public_get_webp(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<Uuid>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT webp_key FROM images WHERE id = $1 AND status IN ('active', 'ready')",
    )
    .bind(image_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("WebP query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (webp_key,) = row;
    let webp_key = webp_key.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({"error": "WebP not yet generated"})))
    })?;

    let storage = pichost_core::storage::local::LocalStorage::new(
        state.config.storage.local_base_path.clone(),
        state.config.server.public_url.clone(),
    );
    let bytes = storage.get(&webp_key).await.map_err(|e| {
        tracing::warn!("WebP storage read failed: {e}");
        (StatusCode::NOT_FOUND, Json(json!({"error": "WebP not found"})))
    })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/webp")
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

/// DELETE /api/v1/images/{id} — delete image + storage files (protected)
pub async fn delete_image(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        r#"SELECT storage_key, storage_backend, thumbnail_key, webp_key
           FROM images WHERE id = $1 AND (user_id = $2 OR $3)"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(user.is_admin)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Delete image query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (storage_key, _storage_backend, thumb_key, webp_key) = row;

    let storage = pichost_core::storage::local::LocalStorage::new(
        state.config.storage.local_base_path.clone(),
        state.config.server.public_url.clone(),
    );

    let _ = storage.delete(&storage_key).await;
    if let Some(ref tk) = thumb_key { let _ = storage.delete(tk).await; }
    if let Some(ref wk) = webp_key { let _ = storage.delete(wk).await; }

    sqlx::query("DELETE FROM images WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Image delete db failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "failed to delete image"})))
        })?;

    tracing::info!(image_id = %id, user_id = %user.id, "image deleted");
    Ok(Json(json!({"message": "image deleted", "id": id})))
}
