use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use pichost_core::models::{ChangePasswordRequest, UpdateProfileRequest, UserProfile};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

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
        r#"SELECT COUNT(*)::BIGINT as total_images,
                  COALESCE(SUM(file_size), 0)::BIGINT as total_size
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

/// GET /api/v1/users/me — current user's full profile
pub async fn get_my_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<UserProfile>, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (Uuid, String, Option<String>, String, String, Option<i64>, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, Option<serde_json::Value>)>(
        "SELECT id, username, email, storage_backend, storage_prefix, storage_quota, is_admin, created_at, updated_at, watermark_config FROM users WHERE id = $1"
    )
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("User profile query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    match row {
        Some((id, username, email, storage_backend, storage_prefix, storage_quota, is_admin, created_at, updated_at, watermark_config)) => {
            Ok(Json(UserProfile {
                id,
                username,
                email,
                storage_backend,
                storage_prefix,
                storage_quota,
                is_admin,
                created_at,
                updated_at,
                watermark_config: watermark_config.and_then(|v| {
                    serde_json::from_value::<pichost_core::models::WatermarkConfig>(v).ok()
                }),
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        )),
    }
}

/// PATCH /api/v1/users/me — update own profile
pub async fn update_my_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<UserProfile>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(ref backend) = payload.storage_backend {
        if state.router.get(backend).is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("unknown backend: {}", backend)})),
            ));
        }
    }

    if let Some(ref username) = payload.username {
        let conflict: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE username = $1 AND id != $2)",
        )
        .bind(username)
        .bind(user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Username uniqueness check failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;
        if conflict {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({"error": "username already taken"})),
            ));
        }
    }

    if let Some(ref email) = payload.email {
        let conflict: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE email = $1 AND id != $2)",
        )
        .bind(email)
        .bind(user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Email uniqueness check failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;
        if conflict {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({"error": "email already taken"})),
            ));
        }
    }

    let (wm_provided, wm_value): (bool, Option<serde_json::Value>) = match payload.watermark_config {
        Some(Some(cfg)) => {
            let json = serde_json::to_value(cfg).map_err(|e| {
                tracing::warn!("Watermark config serialization failed: {e}");
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "invalid watermark config"})),
                )
            })?;
            (true, Some(json))
        }
        Some(None) => (true, None),
        None => (false, None),
    };

    sqlx::query(
        "UPDATE users SET \
         username = COALESCE($1, username), \
         email = CASE WHEN $2::boolean THEN $3 ELSE email END, \
         storage_backend = COALESCE($4, storage_backend), \
         watermark_config = CASE WHEN $6::boolean THEN $7::jsonb ELSE watermark_config END, \
         updated_at = now() \
         WHERE id = $5",
    )
    .bind(&payload.username)
    .bind(payload.email.is_some())
    .bind(&payload.email)
    .bind(&payload.storage_backend)
    .bind(user.id)
    .bind(wm_provided)
    .bind(&wm_value)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if let Some(code) = db_err.code() {
                if code == "23505" {
                    return (
                        StatusCode::CONFLICT,
                        Json(serde_json::json!({"error": "username or email already exists"})),
                    );
                }
            }
        }
        tracing::warn!("Profile update failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    let row = sqlx::query_as::<_, (Uuid, String, Option<String>, String, String, Option<i64>, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, Option<serde_json::Value>)>(
        "SELECT id, username, email, storage_backend, storage_prefix, storage_quota, is_admin, created_at, updated_at, watermark_config FROM users WHERE id = $1"
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Profile re-fetch after update failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    Ok(Json(UserProfile {
        id: row.0, username: row.1, email: row.2,
        storage_backend: row.3, storage_prefix: row.4,
        storage_quota: row.5, is_admin: row.6,
        created_at: row.7, updated_at: row.8,
        watermark_config: row.9.and_then(|v| {
            serde_json::from_value::<pichost_core::models::WatermarkConfig>(v).ok()
        }),
    }))
}

/// POST /api/v1/users/me/password — change own password
pub async fn change_my_password(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if payload.new_password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "new password must be at least 8 characters"})),
        ));
    }

    let current_hash: String = sqlx::query_scalar(
        "SELECT password_hash FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Password hash fetch failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?
    .ok_or_else(|| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "user not found"})),
    ))?;

    let parsed_hash = PasswordHash::new(&current_hash).map_err(|e| {
        tracing::warn!("Invalid stored password hash: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;
    Argon2::default()
        .verify_password(payload.current_password.as_bytes(), &parsed_hash)
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "current password incorrect"})),
            )
        })?;

    let salt = SaltString::generate(&mut OsRng);
    let new_hash = Argon2::default()
        .hash_password(payload.new_password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::warn!("Password hashing failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;

    sqlx::query("UPDATE users SET password_hash = $1, updated_at = now() WHERE id = $2")
        .bind(&new_hash)
        .bind(user.id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Password update failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;

    Ok(Json(serde_json::json!({"message": "password updated"})))
}
