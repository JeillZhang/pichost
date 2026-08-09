use std::sync::Arc;
use pichost_core::DbType;

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;
use sqlx::Pool;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use pichost_core::i18n::Language;
use pichost_core::models::{ChangePasswordRequest, UpdateProfileRequest, UserProfile};

use crate::app::AppState;
use crate::cache::Cache;
use crate::i18n_ext::{error_json, error_json_args, JsonBody, Locale};
use crate::middleware::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct UserStats {
    pub total_images: i64,
    pub total_size: i64,
    pub backend: String,
    pub storage_quota: Option<i64>,
}

/// GET /api/v1/users/me/stats — usage statistics (protected, cached)
pub async fn get_my_stats<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
) -> Result<Json<UserStats>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64, Option<i64>): crate::db::DbRow<DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let quota = fetch_user_quota(&state.pool, user.id, locale).await?;

    let cache_stats = state.cache.get_user_stats(&user.id).await.ok().flatten();
    let default_backend = state.router.default_name();
    if let Some(stats) = try_cached_stats(cache_stats, default_backend, quota) {
        return Ok(Json(stats));
    }

    let (total_images, total_size) = query_user_stats(&state.pool, user.id, locale).await?;

    let stats = UserStats {
        total_images,
        total_size,
        backend: default_backend.to_string(),
        storage_quota: quota,
    };

    populate_user_stats_cache(&state.cache, &user.id, total_images, total_size).await;

    Ok(Json(stats))
}

async fn fetch_user_quota<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<Option<i64>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_scalar("SELECT storage_quota FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Quota query failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
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

async fn query_user_stats<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<(i64, i64), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64, Option<i64>): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*) as total_images,
                  CAST(COALESCE(SUM(file_size), 0) AS BIGINT) as total_size
           FROM images WHERE user_id = $1"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Stats query failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
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

async fn fetch_profile_row<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    log_msg: &str,
    locale: Language,
) -> Result<Option<ProfileRow>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_as::<_, ProfileRow>(PROFILE_SELECT)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::warn!("{log_msg}: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })
}

/// GET /api/v1/users/me — current user's full profile
pub async fn get_my_profile<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
) -> Result<Json<UserProfile>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let row = fetch_profile_row(&state.pool, user.id, "User profile query failed", locale)
        .await?
        .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "user.not_found"))?;
    Ok(Json(build_user_profile(row)))
}

fn ensure_backend_exists<DB: DbType>(
    state: &AppState<DB>,
    payload: &UpdateProfileRequest,
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Some(ref backend) = payload.storage_backend {
        if state.router.get(backend).is_none() {
            return Err(error_json_args(
                locale,
                StatusCode::BAD_REQUEST,
                "user.unknown_backend",
                std::slice::from_ref(backend),
            ));
        }
    }
    Ok(())
}

async fn ensure_username_available<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    username: Option<&str>,
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (bool,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
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
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })?;
    if conflict {
        return Err(error_json(
            locale,
            StatusCode::CONFLICT,
            "user.username_taken",
        ));
    }
    Ok(())
}

async fn ensure_email_available<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    email: Option<&str>,
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (bool,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
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
                error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
            })?;
    if conflict {
        return Err(error_json(locale, StatusCode::CONFLICT, "user.email_taken"));
    }
    Ok(())
}

/// Serialize watermark_config into JSONB with absent/null/value semantics.
fn serialize_watermark_config(
    payload: &UpdateProfileRequest,
    locale: Language,
) -> Result<(bool, Option<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    match &payload.watermark_config {
        Some(Some(cfg)) => {
            let json = serde_json::to_value(cfg).map_err(|e| {
                tracing::warn!("Watermark config serialization failed: {e}");
                error_json(locale, StatusCode::BAD_REQUEST, "user.watermark_invalid")
            })?;
            Ok((true, Some(json)))
        }
        Some(None) => Ok((true, None)),
        None => Ok((false, None)),
    }
}

async fn apply_profile_update<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    payload: &UpdateProfileRequest,
    wm_provided: bool,
    wm_value: &Option<serde_json::Value>,
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query(
        "UPDATE users SET \
         username = COALESCE($1, username), \
         email = CASE WHEN $2 THEN $3 ELSE email END, \
         storage_backend = COALESCE($4, storage_backend), \
         watermark_config = CASE WHEN $6 THEN $7 ELSE watermark_config END, \
         updated_at = CURRENT_TIMESTAMP \
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
        if pichost_core::db::db_error_kind(&e) == pichost_core::db::DbErrorKind::UniqueViolation {
            return error_json(
                locale,
                StatusCode::CONFLICT,
                "user.username_email_taken",
            );
        }
        tracing::warn!("Profile update failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })?;
    Ok(())
}

/// PATCH /api/v1/users/me — update own profile
pub async fn update_my_profile<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
    JsonBody(payload): JsonBody<UpdateProfileRequest>,
) -> Result<Json<UserProfile>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (bool,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    ensure_backend_exists(&state, &payload, locale)?;
    ensure_username_available(&state.pool, user.id, payload.username.as_deref(), locale).await?;
    ensure_email_available(&state.pool, user.id, payload.email.as_deref(), locale).await?;
    let (wm_provided, wm_value) = serialize_watermark_config(&payload, locale)?;
    apply_profile_update(&state.pool, user.id, &payload, wm_provided, &wm_value, locale).await?;
    let row = fetch_profile_row(&state.pool, user.id, "Profile re-fetch after update failed", locale)
        .await?
        .ok_or_else(|| {
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })?;
    Ok(Json(build_user_profile(row)))
}

async fn fetch_password_hash<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<String, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (std::string::String,): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let current_hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                tracing::warn!("Password hash fetch failed: {e}");
                error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
            })?;
    current_hash.ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "user.not_found"))
}

fn verify_current_password(
    current_password: &str,
    stored_hash: &str,
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let parsed_hash = PasswordHash::new(stored_hash).map_err(|e| {
        tracing::warn!("Invalid stored password hash: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })?;
    Argon2::default()
        .verify_password(current_password.as_bytes(), &parsed_hash)
        .map_err(|_| {
            error_json(
                locale,
                StatusCode::UNAUTHORIZED,
                "user.current_password_incorrect",
            )
        })
}

fn hash_new_password(
    new_password: &str,
    locale: Language,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(new_password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::warn!("Password hashing failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })
}

async fn update_password_hash<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    new_hash: &str,
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query("UPDATE users SET password_hash = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2")
        .bind(new_hash)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Password update failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })?;
    Ok(())
}

/// POST /api/v1/users/me/password — change own password
pub async fn change_my_password<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
    JsonBody(payload): JsonBody<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (std::string::String,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    if payload.new_password.len() < 8 {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "user.password_too_weak",
        ));
    }
    let current_hash = fetch_password_hash(&state.pool, user.id, locale).await?;
    verify_current_password(&payload.current_password, &current_hash, locale)?;
    let new_hash = hash_new_password(&payload.new_password, locale)?;
    update_password_hash(&state.pool, user.id, &new_hash, locale).await?;
    Ok(Json(serde_json::json!({"message": "password updated"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn profile_row(watermark: Option<serde_json::Value>) -> ProfileRow {
        (
            Uuid::new_v4(),
            "alice".into(),
            Some("a@b.c".into()),
            "local".into(),
            "pfx".into(),
            Some(1024),
            true,
            chrono::Utc::now(),
            chrono::Utc::now(),
            watermark,
        )
    }

    fn stats_map(images: &str, size: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("total_images".into(), images.into());
        map.insert("total_size".into(), size.into());
        map
    }

    #[test]
    fn test_try_cached_stats_complete() {
        let stats = try_cached_stats(Some(stats_map("5", "1024")), "local", Some(2048)).unwrap();
        assert_eq!(stats.total_images, 5);
        assert_eq!(stats.total_size, 1024);
        assert_eq!(stats.backend, "local");
        assert_eq!(stats.storage_quota, Some(2048));
    }

    #[test]
    fn test_try_cached_stats_missing_field() {
        let mut map = HashMap::new();
        map.insert("total_size".into(), "1024".into());
        assert!(try_cached_stats(Some(map), "local", None).is_none());
        assert!(try_cached_stats(None, "local", None).is_none());
    }

    #[test]
    fn test_try_cached_stats_unparseable() {
        assert!(try_cached_stats(Some(stats_map("abc", "1024")), "local", None).is_none());
    }

    #[test]
    fn test_build_user_profile_with_watermark() {
        let wm = serde_json::json!({"enabled": true, "text": "@alice"});
        let profile = build_user_profile(profile_row(Some(wm)));
        assert_eq!(profile.username, "alice");
        assert_eq!(profile.email.as_deref(), Some("a@b.c"));
        assert_eq!(profile.storage_quota, Some(1024));
        let cfg = profile.watermark_config.unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.text, "@alice");
    }

    #[test]
    fn test_build_user_profile_without_watermark() {
        let profile = build_user_profile(profile_row(None));
        assert!(profile.watermark_config.is_none());
    }

    fn wm_payload(watermark_config: Option<Option<pichost_core::models::WatermarkConfig>>) -> UpdateProfileRequest {
        UpdateProfileRequest {
            username: None,
            email: None,
            storage_backend: None,
            watermark_config,
        }
    }

    #[test]
    fn test_serialize_watermark_config_some() {
        let cfg = pichost_core::models::WatermarkConfig {
            enabled: true,
            text: "x".into(),
            font: "NotoSansSC-Regular".into(),
            font_size: 48,
            color: "rgba(255, 255, 255, 0.5)".into(),
            rotation: -30.0,
            scale: 0.15,
            position: pichost_core::models::WatermarkPosition::BottomRight,
            margin_x: 20,
            margin_y: 20,
        };
        let (provided, value) =
            serialize_watermark_config(&wm_payload(Some(Some(cfg))), Language::En).unwrap();
        assert!(provided);
        assert!(value.is_some());
    }

    #[test]
    fn test_serialize_watermark_config_clear() {
        let (provided, value) =
            serialize_watermark_config(&wm_payload(Some(None)), Language::En).unwrap();
        assert!(provided);
        assert!(value.is_none());
    }

    #[test]
    fn test_serialize_watermark_config_absent() {
        let (provided, value) = serialize_watermark_config(&wm_payload(None), Language::En).unwrap();
        assert!(!provided);
        assert!(value.is_none());
    }

    #[test]
    fn test_verify_current_password() {
        let hash = hash_new_password("correct-horse", Language::En).unwrap();
        assert!(verify_current_password("correct-horse", &hash, Language::En).is_ok());
        let err = verify_current_password("wrong-password", &hash, Language::En).unwrap_err();
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        let err = verify_current_password("x", "not-a-hash", Language::En).unwrap_err();
        assert_eq!(err.0, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_hash_new_password() {
        let hash = hash_new_password("new-password-123", Language::En).unwrap();
        assert!(hash.starts_with("$argon2"));
    }
}
