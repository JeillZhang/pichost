use std::sync::Arc;

use axum::{
    extract::{Extension, Multipart, Path, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use pichost_core::StorageRouter;
use serde_json::json;
use uuid::Uuid;

use crate::app::AppState;
use crate::db::DbPool;
use crate::middleware::auth::AuthUser;
use crate::services::upload::{self, ImageListQuery, ImageListResponse, ImageRow, UploadResult};

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn check_image_status(status: &str) -> bool {
    status == "active" || status == "ready"
}

fn validate_batch_ids(ids: &[Uuid]) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "no image ids provided"})),
        ));
    }
    if ids.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "batch limit is 100 images"})),
        ));
    }
    Ok(())
}

async fn cleanup_storage_files(
    router: &StorageRouter,
    backend: &str,
    storage_key: &str,
    thumb_key: &Option<String>,
    webp_key: &Option<String>,
) {
    let storage = router.for_backend(backend);
    let _ = storage.delete(storage_key).await;
    if let Some(ref tk) = thumb_key {
        let _ = storage.delete(tk).await;
    }
    if let Some(ref wk) = webp_key {
        let _ = storage.delete(wk).await;
    }
}

type RouteError = (StatusCode, Json<serde_json::Value>);

async fn count_user_images(
    pool: &DbPool,
    user_id: Uuid,
    search_term: &str,
) -> Result<i64, RouteError> {
    let log_err = |e: sqlx::Error| {
        tracing::warn!("Image count query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal server error"})),
        )
    };
    if search_term.is_empty() {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM images WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(log_err)
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM images WHERE user_id = $1 AND original_name ILIKE $2",
        )
        .bind(user_id)
        .bind(format!("%{}%", search_term))
        .fetch_one(pool)
        .await
        .map_err(log_err)
    }
}

async fn fetch_user_images(
    pool: &DbPool, user_id: Uuid, sort_col: &str, order_dir: &str,
    search_term: &str, limit: i64, offset: i64,
) -> Result<Vec<ImageRow>, RouteError> {
    let map_err = |e: sqlx::Error| {
        tracing::warn!("Image list query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
    };
    let base = "SELECT id,public_key,original_name,url,mime_type,file_size,\
                sha256,width,height,status,thumbnail_url,webp_url,created_at FROM images";
    if search_term.is_empty() {
        let sql = format!("{base} WHERE user_id = $1 ORDER BY {sort_col} {order_dir} LIMIT $2 OFFSET $3");
        sqlx::query_as::<_, ImageRow>(&sql).bind(user_id).bind(limit).bind(offset).fetch_all(pool).await.map_err(map_err)
    } else {
        let sql = format!("{base} WHERE user_id = $1 AND original_name ILIKE $2 ORDER BY {sort_col} {order_dir} LIMIT $3 OFFSET $4");
        sqlx::query_as::<_, ImageRow>(&sql).bind(user_id).bind(format!("%{}%", search_term)).bind(limit).bind(offset).fetch_all(pool).await.map_err(map_err)
    }
}

fn map_rows_to_results(rows: Vec<ImageRow>) -> Vec<UploadResult> {
    rows.into_iter().map(UploadResult::from_row).collect()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/images — upload an image (protected)
pub async fn upload_handler(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResult>), (StatusCode, Json<serde_json::Value>)> {
    match upload::process_upload(state, user, multipart).await {
        Ok(result) => {
            crate::metrics::UPLOADS_TOTAL.inc();
            Ok((StatusCode::CREATED, Json(result)))
        }
        Err(e) => {
            crate::metrics::UPLOAD_ERRORS_TOTAL.inc();
            Err(e)
        }
    }
}

/// GET /api/v1/images — list user's images with pagination, search, and sort (protected)
pub async fn list_images(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Query(params): axum::extract::Query<ImageListQuery>,
) -> Result<Json<ImageListResponse>, RouteError> {
    let page = params.page.max(1);
    let per_page = params.per_page.clamp(1, 100);
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    let sort_col = match params.sort.as_str() {
        "created_at" | "file_size" | "original_name" => params.sort.as_str(),
        _ => "created_at",
    };
    let order_dir = match params.order.as_str() {
        "asc" | "ASC" => "ASC",
        _ => "DESC",
    };

    let search_term = params.search.trim();
    let total = count_user_images(&state.pool, user.id, search_term).await?;
    let rows = fetch_user_images(
        &state.pool, user.id, sort_col, order_dir, search_term, limit, offset,
    )
    .await?;
    let items = map_rows_to_results(rows);

    let total_pages = if total == 0 {
        1
    } else {
        ((total as f64) / (per_page as f64)).ceil() as u32
    };

    Ok(Json(ImageListResponse {
        items,
        total,
        page,
        per_page,
        total_pages,
    }))
}

/// GET /api/v1/images/{id} — single image detail (protected, cached)
pub async fn get_image(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<UploadResult>, RouteError> {
    let result = state
        .cache
        .cached_meta(&id, 600, async {
            sqlx::query_as::<_, ImageRow>(
                "SELECT id, public_key, original_name, url, mime_type, file_size,\
                 sha256, width, height, status, thumbnail_url, webp_url, created_at \
                 FROM images WHERE id = $1 AND user_id = $2",
            )
            .bind(id)
            .bind(user.id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                tracing::warn!("Get image query failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "internal error"})),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "image not found"})),
                )
            })
            .map(UploadResult::from_row)
        })
        .await?;

    Ok(Json(result))
}

/// GET /u/{public_key} — serve image publicly (unauthenticated)
pub async fn public_get(
    State(state): State<Arc<AppState>>,
    Path(public_key): Path<String>,
) -> Result<Response, RouteError> {
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT storage_key, mime_type, status, storage_backend FROM images WHERE public_key = $1",
    )
    .bind(&public_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Public image query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (storage_key, mime_type, status, storage_backend) = row;
    if !check_image_status(&status) {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))));
    }

    let storage = state.router.for_backend(&storage_backend);
    let bytes = storage.get(&storage_key).await.map_err(|e| {
        tracing::warn!("Storage read failed on {}: {e}", storage.backend_name());
        (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"})))
    })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &mime_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

fn mime_for_thumb_key(key: &str) -> &'static str {
    if key.ends_with(".png") {
        "image/png"
    } else {
        "image/jpeg"
    }
}

/// GET /u/thumb/{image_id} — serve generated thumbnail (unauthenticated)
pub async fn public_get_thumb(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<Uuid>,
) -> Result<Response, RouteError> {
    let row = sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT thumbnail_key, storage_backend FROM images WHERE id = $1 AND status IN ('active', 'ready')",
    )
    .bind(image_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Thumb query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (thumb_key, storage_backend) = row;
    let thumb_key = thumb_key.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "thumbnail not yet generated"})),
        )
    })?;

    let bytes = state
        .cache
        .cached_thumb(&format!("thumb:{}", image_id), 3600, async {
            let backend = state.router.for_backend(&storage_backend);
            backend.get(&thumb_key).await.map_err(|e| {
                tracing::warn!("Thumb storage read failed: {e}");
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "thumbnail not found"})),
                )
            })
        })
        .await?;

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
) -> Result<Response, RouteError> {
    let row = sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT webp_key, storage_backend FROM images WHERE id = $1 AND status IN ('active', 'ready')",
    )
    .bind(image_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("WebP query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (webp_key, storage_backend) = row;
    let webp_key = webp_key.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "WebP not yet generated"})),
        )
    })?;

    let bytes = state
        .cache
        .cached_thumb(&format!("webp:{}", image_id), 3600, async {
            let backend = state.router.for_backend(&storage_backend);
            backend.get(&webp_key).await.map_err(|e| {
                tracing::warn!("WebP storage read failed: {e}");
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "WebP not found"})),
                )
            })
        })
        .await?;

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
) -> Result<Json<serde_json::Value>, RouteError> {
    let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        r#"SELECT storage_key, storage_backend, thumbnail_key, webp_key
           FROM images WHERE id = $1 AND (user_id = $2 OR $3)"#,
    )
    .bind(id).bind(user.id).bind(user.is_admin)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Delete image query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (storage_key, storage_backend, thumb_key, webp_key) = row;
    cleanup_storage_files(&state.router, &storage_backend, &storage_key, &thumb_key, &webp_key).await;

    sqlx::query("DELETE FROM images WHERE id = $1")
        .bind(id).execute(&state.pool).await
        .map_err(|e| {
            tracing::warn!("Image delete db failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "failed to delete image"})))
        })?;

    tracing::info!(image_id = %id, user_id = %user.id, "image deleted");
    Ok(Json(json!({"message": "image deleted", "id": id})))
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BatchDeleteRequest {
    pub ids: Vec<Uuid>,
}

/// POST /api/v1/images/batch-delete — delete multiple images (protected)
pub async fn batch_delete(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<BatchDeleteRequest>,
) -> Result<Json<serde_json::Value>, RouteError> {
    validate_batch_ids(&body.ids)?;

    let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT storage_key, storage_backend, thumbnail_key, webp_key
           FROM images WHERE id = ANY($1) AND (user_id = $2 OR $3)"#,
    )
    .bind(&body.ids).bind(user.id).bind(user.is_admin)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Batch delete query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
    })?;

    for (sk, sb, tk, wk) in &rows {
        cleanup_storage_files(&state.router, sb, sk, tk, wk).await;
    }

    let deleted = sqlx::query("DELETE FROM images WHERE id = ANY($1)")
        .bind(&body.ids).execute(&state.pool).await
        .map_err(|e| {
            tracing::warn!("Batch delete DB failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "failed to delete images"})))
        })?
        .rows_affected() as usize;

    let failed = body.ids.len().saturating_sub(deleted);
    tracing::info!(user_id = %user.id, requested = body.ids.len(), deleted, failed, "batch delete");
    Ok(Json(json!({"message": "batch delete completed", "deleted": deleted, "failed": failed})))
}
