use std::collections::HashMap;
use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;

use crate::app::AppState;

#[derive(Debug, Serialize)]
pub struct BackendStats {
    pub total_images: i64,
    pub total_size: i64,
}

#[derive(Debug, Serialize)]
pub struct AdminStats {
    pub total_users: i64,
    pub total_images: i64,
    pub total_size: i64,
    pub active_users_24h: i64,
    pub storage_backends: HashMap<String, BackendStats>,
}

/// GET /api/v1/admin/stats — system-wide statistics (admin only, cached 5 min)
pub async fn get_admin_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminStats>, (StatusCode, Json<serde_json::Value>)> {
    // Try cache first using nil UUID as admin stats key
    if let Ok(Some(stats_map)) = state.cache.get_user_stats(&uuid::Uuid::nil()).await {
        if let (Some(total_users), Some(total_images), Some(total_size)) = (
            stats_map.get("total_users").and_then(|v| v.parse().ok()),
            stats_map.get("total_images").and_then(|v| v.parse().ok()),
            stats_map.get("total_size").and_then(|v| v.parse().ok()),
        ) {
            let active_users_24h: i64 = stats_map
                .get("active_users_24h")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let local = BackendStats {
                total_images: stats_map
                    .get("local_images")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(total_images),
                total_size: stats_map
                    .get("local_size")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(total_size),
            };
            let rustfs = BackendStats {
                total_images: stats_map
                    .get("rustfs_images")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                total_size: stats_map
                    .get("rustfs_size")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            };
            let mut backends = HashMap::new();
            backends.insert("local".to_string(), local);
            backends.insert("rustfs".to_string(), rustfs);

            return Ok(Json(AdminStats {
                total_users,
                total_images,
                total_size,
                active_users_24h,
                storage_backends: backends,
            }));
        }
    }

    // Cache miss — query DB
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin stats user count failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;

    let img_row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*) as total_images, COALESCE(SUM(file_size), 0) as total_size
           FROM images"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin stats image query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
    })?;
    let (total_images, total_size) = (img_row.0, img_row.1.unwrap_or(0));

    // Active users in last 24h
    let active_users_24h: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT user_id) FROM images
           WHERE created_at > NOW() - INTERVAL '24 hours'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    // Per-backend breakdown
    let local_row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*), COALESCE(SUM(file_size), 0)
           FROM images WHERE storage_backend = 'local'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, None));

    let rustfs_row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*), COALESCE(SUM(file_size), 0)
           FROM images WHERE storage_backend = 'rustfs'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, None));

    let mut backends = HashMap::new();
    backends.insert(
        "local".to_string(),
        BackendStats {
            total_images: local_row.0,
            total_size: local_row.1.unwrap_or(0),
        },
    );
    backends.insert(
        "rustfs".to_string(),
        BackendStats {
            total_images: rustfs_row.0,
            total_size: rustfs_row.1.unwrap_or(0),
        },
    );

    let stats = AdminStats {
        total_users,
        total_images,
        total_size,
        active_users_24h,
        storage_backends: backends,
    };

    // Populate cache (best-effort)
    let nil_uuid = uuid::Uuid::nil();
    let _ = state.cache.incr_user_stat(&nil_uuid, "total_users", total_users).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "total_images", total_images).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "total_size", total_size).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "active_users_24h", active_users_24h).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "local_images", local_row.0).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "local_size", local_row.1.unwrap_or(0)).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "rustfs_images", rustfs_row.0).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "rustfs_size", rustfs_row.1.unwrap_or(0)).await;

    Ok(Json(stats))
}
