use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use pichost_core::models::{ChangePasswordRequest, UpdateProfileRequest, UserProfile};

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
    // Missing or unparseable fields mean the cache entry is empty/stale —
    // return None so the caller queries the DB instead of serving zeros.
    // (HGETALL on a nonexistent key yields an empty map, not None.)
    let total_images = stats_map.get("total_images").and_then(|v| v.parse().ok())?;
    let total_size = stats_map.get("total_size").and_then(|v| v.parse().ok())?;
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
    // Overwrite (HSET) — repopulated from DB on cache miss; HINCRBY
    // would accumulate and double-count.
    let _ = cache
        .set_user_stats(
            user_id,
            &[
                ("total_images", Some(total_images)),
                ("total_size", Some(total_size)),
            ],
        )
        .await;
}

type ProfileRow = (
    Uuid,
    String,
    Option<String>,
    String,
    String,
    Option<i64>,
    bool,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    Option<serde_json::Value>,
);

const PROFILE_SELECT: &str = "SELECT id, username, email, storage_backend, storage_prefix, \
    storage_quota, is_admin, created_at, updated_at, watermark_config FROM users WHERE id = $1";

fn build_user_profile(row: ProfileRow) -> UserProfile {
    UserProfile {
        id: row.0,
        username: row.1,
        email: row.2,
        storage_backend: row.3,
        storage_prefix: row.4,
        storage_quota: row.5,
        is_admin: row.6,
        created_at: row.7,
        updated_at: row.8,
        watermark_config: row.9.and_then(|v| {
            serde_json::from_value::<pichost_core::models::WatermarkConfig>(v).ok()
        }),
    }
}

async fn fetch_profile_row(
    pool: &PgPool,
    user_id: Uuid,
    log_msg: &str,
) -> Result<Option<ProfileRow>, (StatusCode, Json<serde_json::Value>)> {
    sqlx::query_as::<_, ProfileRow>(PROFILE_SELECT)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::warn!("{log_msg}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })
}

/// GET /api/v1/users/me — current user's full profile
pub async fn get_my_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<UserProfile>, (StatusCode, Json<serde_json::Value>)> {
    let row = fetch_profile_row(&state.pool, user.id, "User profile query failed")
        .await?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "user not found"})),
            )
        })?;
    Ok(Json(build_user_profile(row)))
}

fn ensure_backend_exists(
    state: &AppState,
    payload: &UpdateProfileRequest,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Some(ref backend) = payload.storage_backend {
        if state.router.get(backend).is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("unknown backend: {}", backend)})),
            ));
        }
    }
    Ok(())
}

async fn ensure_username_available(
    pool: &PgPool,
    user_id: Uuid,
    username: Option<&str>,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let Some(username) = username else {
        return Ok(());
    };
    let conflict: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE username = $1 AND id != $2)",
    )
    .bind(username)
    .bind(user_id)
    .fetch_one(pool)
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
    Ok(())
}

async fn ensure_email_available(
    pool: &PgPool,
    user_id: Uuid,
    email: Option<&str>,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let Some(email) = email else {
        return Ok(());
    };
    let conflict: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1 AND id != $2)")
            .bind(email)
            .bind(user_id)
            .fetch_one(pool)
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
    Ok(())
}

/// Serialize watermark_config into JSONB with absent/null/value semantics.
fn serialize_watermark_config(
    payload: &UpdateProfileRequest,
) -> Result<(bool, Option<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    match &payload.watermark_config {
        Some(Some(cfg)) => {
            let json = serde_json::to_value(cfg).map_err(|e| {
                tracing::warn!("Watermark config serialization failed: {e}");
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "invalid watermark config"})),
                )
            })?;
            Ok((true, Some(json)))
        }
        Some(None) => Ok((true, None)),
        None => Ok((false, None)),
    }
}

async fn apply_profile_update(
    pool: &PgPool,
    user_id: Uuid,
    payload: &UpdateProfileRequest,
    wm_provided: bool,
    wm_value: &Option<serde_json::Value>,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
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
    .bind(user_id)
    .bind(wm_provided)
    .bind(wm_value)
    .execute(pool)
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
    Ok(())
}

/// PATCH /api/v1/users/me — update own profile
pub async fn update_my_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<UserProfile>, (StatusCode, Json<serde_json::Value>)> {
    ensure_backend_exists(&state, &payload)?;
    ensure_username_available(&state.pool, user.id, payload.username.as_deref()).await?;
    ensure_email_available(&state.pool, user.id, payload.email.as_deref()).await?;
    let (wm_provided, wm_value) = serialize_watermark_config(&payload)?;
    apply_profile_update(&state.pool, user.id, &payload, wm_provided, &wm_value).await?;
    let row = fetch_profile_row(&state.pool, user.id, "Profile re-fetch after update failed")
        .await?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;
    Ok(Json(build_user_profile(row)))
}

async fn fetch_password_hash(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let current_hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                tracing::warn!("Password hash fetch failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                )
            })?;
    current_hash.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        )
    })
}

fn verify_current_password(
    current_password: &str,
    stored_hash: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let parsed_hash = PasswordHash::new(stored_hash).map_err(|e| {
        tracing::warn!("Invalid stored password hash: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;
    Argon2::default()
        .verify_password(current_password.as_bytes(), &parsed_hash)
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "current password incorrect"})),
            )
        })
}

fn hash_new_password(
    new_password: &str,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(new_password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::warn!("Password hashing failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })
}

async fn update_password_hash(
    pool: &PgPool,
    user_id: Uuid,
    new_hash: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    sqlx::query("UPDATE users SET password_hash = $1, updated_at = now() WHERE id = $2")
        .bind(new_hash)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Password update failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;
    Ok(())
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
    let current_hash = fetch_password_hash(&state.pool, user.id).await?;
    verify_current_password(&payload.current_password, &current_hash)?;
    let new_hash = hash_new_password(&payload.new_password)?;
    update_password_hash(&state.pool, user.id, &new_hash).await?;
    Ok(Json(serde_json::json!({"message": "password updated"})))
}
