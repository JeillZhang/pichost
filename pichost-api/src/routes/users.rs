use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;

use crate::app::AppState;
use crate::middleware::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct UserStats {
    pub total_images: i64,
    pub total_size: i64,
    pub backend: String,
}

/// GET /api/v1/users/me/stats — usage statistics (protected, cached)
pub async fn get_my_stats(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<UserStats>, (StatusCode, Json<serde_json::Value>)> {
    // Try cache first
    if let Ok(Some(stats_map)) = state.cache.get_user_stats(&user.id).await {
        let total_images = stats_map
            .get("total_images")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let total_size = stats_map
            .get("total_size")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        return Ok(Json(UserStats {
            total_images,
            total_size,
            backend: state.router.default_name().to_string(),
        }));
    }

    // Cache miss — query DB
    let row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*) as total_images,
                  COALESCE(SUM(file_size), 0) as total_size
           FROM images WHERE user_id = $1"#,
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Stats query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    let stats = UserStats {
        total_images: row.0,
        total_size: row.1.unwrap_or(0),
        backend: state.router.default_name().to_string(),
    };

    // Populate cache (best-effort)
    let _ = state
        .cache
        .incr_user_stat(&user.id, "total_images", stats.total_images)
        .await;
    let _ = state
        .cache
        .incr_user_stat(&user.id, "total_size", stats.total_size)
        .await;

    Ok(Json(stats))
}
