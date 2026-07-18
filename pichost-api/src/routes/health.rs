use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::app::AppState;

/// GET /api/health — service health check (public, no auth)
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Check PostgreSQL
    let pg_ok = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();

    // Check Redis
    let redis_ok = state.cache.get("health:ping").await.is_ok();

    let status = if pg_ok && redis_ok {
        "healthy"
    } else {
        "degraded"
    };

    let http_status = if pg_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        http_status,
        Json(serde_json::json!({
            "status": status,
            "components": {
                "postgres": {
                    "status": if pg_ok { "ok" } else { "error" }
                },
                "redis": {
                    "status": if redis_ok { "ok" } else { "error" }
                },
                "storage": {
                    "status": "ok",
                    "detail": format!(
                        "default_backend={}, registered={}",
                        state.router.default_name(),
                        state.router.backend_count()
                    )
                }
            }
        })),
    )
}
