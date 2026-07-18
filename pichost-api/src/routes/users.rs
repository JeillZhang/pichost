use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::app::AppState;
use crate::cache::Cache;
use crate::middleware::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct UserStats {
    pub total_images: i64,
    pub total_size: i64,
    pub backend: String,
    pub storage_quota: Option<i64>,
}

/// GET /api/v1/users/me/stats — usage statistics (protected, cached)
pub async fn get_my_stats(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<UserStats>, (StatusCode, Json<serde_json::Value>)> {
    let quota = fetch_user_quota(&state.pool, user.id).await?;

    let cache_stats = state.cache.get_user_stats(&user.id).await.ok().flatten();
    let default_backend = state.router.default_name();
    if let Some(stats) = try_cached_stats(cache_stats, default_backend, quota) {
        return Ok(Json(stats));
    }

    let (total_images, total_size) = query_user_stats(&state.pool, user.id).await?;

    let stats = UserStats {
        total_images,
        total_size,
        backend: default_backend.to_string(),
        storage_quota: quota,
    };

    populate_user_stats_cache(&state.cache, &user.id, total_images, total_size).await;

    Ok(Json(stats))
}

async fn fetch_user_quota(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<i64>, (StatusCode, Json<serde_json::Value>)> {
    sqlx::query_scalar("SELECT storage_quota FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Quota query failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })
        .map(|r| r.flatten())
}

fn try_cached_stats(
    cache_stats: Option<std::collections::HashMap<String, String>>,
    default_backend: &str,
    quota: Option<i64>,
) -> Option<UserStats> {
    let stats_map = cache_stats?;
    let total_images = stats_map
        .get("total_images")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let total_size = stats_map
        .get("total_size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Some(UserStats {
        total_images,
        total_size,
        backend: default_backend.to_string(),
        storage_quota: quota,
    })
}

async fn query_user_stats(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<(i64, i64), (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*) as total_images,
                  COALESCE(SUM(file_size), 0) as total_size
           FROM images WHERE user_id = $1"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Stats query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    Ok((row.0, row.1.unwrap_or(0)))
}

async fn populate_user_stats_cache(
    cache: &Cache,
    user_id: &Uuid,
    total_images: i64,
    total_size: i64,
) {
    let _ = cache
        .incr_user_stat(user_id, "total_images", total_images)
        .await;
    let _ = cache
        .incr_user_stat(user_id, "total_size", total_size)
        .await;
}
