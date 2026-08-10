use pichost_core::DbType;
use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use pichost_core::i18n::{I18n, Language};
use pichost_core::StorageRouter;

fn deserialize_optional_jsonb<'de, D>(
    deserializer: D,
) -> Result<Option<Option<serde_json::Value>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

use crate::app::AppState;
use crate::cache::InviteCodeInfo;
use crate::i18n_ext::{error_json, error_json_args, JsonBody, Locale};
use crate::metrics::{TOTAL_IMAGES, TOTAL_STORAGE_BYTES, TOTAL_USERS};
use crate::middleware::auth::AuthUser;
use crate::routes::auth::UserInfo;
use crate::services::config::{self, SystemConfig};
use sqlx::Pool;

// ── Error shorthand ──

type AdminError = (StatusCode, Json<serde_json::Value>);

fn internal_error(locale: Language) -> AdminError {
    error_json(
        locale,
        StatusCode::INTERNAL_SERVER_ERROR,
        "common.internal_error",
    )
}

// ── Invite Code types ──

#[derive(Debug, Deserialize)]
pub struct CreateInviteBody {
    pub ttl_days: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct CreateInviteResponse {
    pub code: String,
    pub expires_at: i64,
}

// ---- User management types ----

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<UserInfo>,
    pub total: i64,
}

// ── Helper types ───────────────────────────────────────────────────────

type UserRow = (
    Uuid,
    String,
    Option<String>,
    bool,
    String,
    chrono::DateTime<chrono::Utc>,
    Option<i64>,
    Option<serde_json::Value>,
);

// ── Config endpoint types ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpdateConfigBody {
    pub database_url: Option<String>,
    pub redis_url: Option<String>,
    pub public_url: Option<String>,
    pub default_backend: Option<String>,
    pub local_base_path: Option<String>,
    pub i18n: Option<UpdateConfigI18n>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigI18n {
    pub language: Option<String>,
    pub locales_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TestConfigBody {
    pub database_url: Option<String>,
    pub redis_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RestoreBackupBody {
    pub backup_file: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub token_encryption_key: String,
    pub public_url: String,
    pub default_backend: String,
    pub local_base_path: String,
    pub config_path: String,
    pub i18n: ConfigResponseI18n,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponseI18n {
    pub language: String,
    pub locales_dir: String,
}

#[derive(Debug, Serialize)]
pub struct BackupInfo {
    pub filename: String,
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub database: Option<String>,
    pub redis: Option<String>,
}

struct UserUpdateParams<'a, DB: DbType> {
    pool: &'a Pool<DB>,
    user_id: Uuid,
    username: &'a str,
    email: &'a Option<String>,
    is_admin: bool,
    storage_backend: &'a str,
    storage_quota: Option<i64>,
    watermark_config: Option<serde_json::Value>,
    password_hash: Option<&'a str>,
}

struct StatsCacheParams {
    total_users: i64,
    total_images: i64,
    total_size: i64,
    active_users_24h: i64,
    total_quota: Option<i64>,
    local_images: i64,
    local_size: i64,
    rustfs_size: i64,
}

// ── Helpers ────────────────────────────────────────────────────────────

fn map_user_rows(rows: Vec<UserRow>) -> Vec<UserInfo> {
    rows.into_iter()
        .map(
            |(id, username, email, is_admin, _, _, storage_quota, _)| UserInfo {
                id,
                username,
                email,
                is_admin,
                storage_quota,
            },
        )
        .collect()
}

// ── update_user helpers ────────────────────────────────────────────────

async fn hash_password_if_provided(
    password: &Option<String>,
    locale: Language,
) -> Result<Option<String>, AdminError> {
    let Some(password) = password else {
        return Ok(None);
    };
    if password.len() < 8 {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "admin.password_too_weak",
        ));
    }
    use argon2::password_hash::SaltString;
    use argon2::PasswordHasher;
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    let argon2 = argon2::Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| {
            tracing::warn!("Password hashing failed: {e}");
            internal_error(locale)
        })?
        .to_string();
    Ok(Some(hash))
}

async fn execute_user_update<DB: DbType>(
    params: UserUpdateParams<'_, DB>,
    locale: Language,
) -> Result<(), AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    if let Some(ph) = params.password_hash {
        sqlx::query(
            r#"UPDATE users SET username = $1, email = $2, is_admin = $3,
               storage_backend = $4, password_hash = $5, storage_quota = $6,
               watermark_config = $7 WHERE id = $8"#,
        )
        .bind(params.username)
        .bind(params.email)
        .bind(params.is_admin)
        .bind(params.storage_backend)
        .bind(ph)
        .bind(params.storage_quota)
        .bind(params.watermark_config)
        .bind(params.user_id)
        .execute(params.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin update user (with pw) failed: {e}");
            internal_error(locale)
        })?;
    } else {
        sqlx::query(
            r#"UPDATE users SET username = $1, email = $2, is_admin = $3,
               storage_backend = $4, storage_quota = $5,
               watermark_config = $6 WHERE id = $7"#,
        )
        .bind(params.username)
        .bind(params.email)
        .bind(params.is_admin)
        .bind(params.storage_backend)
        .bind(params.storage_quota)
        .bind(params.watermark_config)
        .bind(params.user_id)
        .execute(params.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin update user failed: {e}");
            internal_error(locale)
        })?;
    }
    Ok(())
}

type UserFields = (
    String,
    Option<String>,
    bool,
    String,
    Option<i64>,
    Option<serde_json::Value>,
);

async fn fetch_user_fields<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<UserFields, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (
        String,
        Option<String>,
        bool,
        String,
        Option<i64>,
        Option<serde_json::Value>,
    ): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_as::<_, UserFields>(
        "SELECT username, email, is_admin, storage_backend, storage_quota, watermark_config \
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin update user query failed: {e}");
        internal_error(locale)
    })?
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "user.not_found"))
}

fn merge_user_fields(body: &UpdateUserBody, existing: UserFields) -> UserFields {
    let (username, email, is_admin, storage_backend, storage_quota, watermark_config) = existing;
    let new_username = body.username.clone().unwrap_or(username);
    let new_email = body.email.clone().or(email);
    let new_is_admin = body.is_admin.unwrap_or(is_admin);
    let new_storage_backend = body.storage_backend.clone().unwrap_or(storage_backend);
    let new_storage_quota = if body.storage_quota == Some(0) {
        None
    } else {
        body.storage_quota.or(storage_quota)
    };
    let new_watermark_config = match body.watermark_config {
        Some(Some(ref v)) => Some(v.clone()),
        Some(None) => None,
        None => watermark_config,
    };
    (
        new_username,
        new_email,
        new_is_admin,
        new_storage_backend,
        new_storage_quota,
        new_watermark_config,
    )
}

async fn fetch_and_merge_user_fields<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    body: &UpdateUserBody,
    locale: Language,
) -> Result<UserFields, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (
        String,
        Option<String>,
        bool,
        String,
        Option<i64>,
        Option<serde_json::Value>,
    ): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let existing = fetch_user_fields(pool, user_id, locale).await?;
    Ok(merge_user_fields(body, existing))
}

// ── delete_user helpers ────────────────────────────────────────────────

async fn collect_and_cleanup_storage_files<DB: DbType>(
    router: &StorageRouter,
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<usize, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let image_keys: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT storage_key, thumbnail_key, webp_key FROM images WHERE user_id = $1"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin delete user image keys query failed: {e}");
        internal_error(locale)
    })?;

    let count = image_keys.len();
    let storage = router.default_backend();
    for (key, thumb_key, webp_key) in &image_keys {
        let _ = storage.delete(key).await;
        if let Some(tk) = thumb_key {
            let _ = storage.delete(tk).await;
        }
        if let Some(wk) = webp_key {
            let _ = storage.delete(wk).await;
        }
    }

    Ok(count)
}

async fn delete_user_from_db<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<(), AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query("DELETE FROM images WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin delete user images failed: {e}");
            internal_error(locale)
        })?;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin delete user failed: {e}");
            internal_error(locale)
        })?;

    Ok(())
}

async fn verify_user_exists<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<(), AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (bool,): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let exists: bool =
        sqlx::query_scalar::<DB, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                tracing::warn!("Admin delete user check failed: {e}");
                internal_error(locale)
            })?;
    if !exists {
        return Err(error_json(locale, StatusCode::NOT_FOUND, "user.not_found"));
    }
    Ok(())
}

// ── get_admin_stats helpers ────────────────────────────────────────────

fn try_parse_cached_stats(stats_map: &HashMap<String, String>) -> Option<AdminStats> {
    let total_users: i64 = stats_map.get("total_users")?.parse().ok()?;
    let total_images: i64 = stats_map.get("total_images")?.parse().ok()?;
    let total_size: i64 = stats_map.get("total_size")?.parse().ok()?;
    let active_users_24h: i64 = stats_map
        .get("active_users_24h")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let parse = |k: &str, fallback| {
        stats_map
            .get(k)
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
    };
    let local = BackendStats {
        total_images: parse("local_images", total_images),
        total_size: parse("local_size", total_size),
    };
    let rustfs = BackendStats {
        total_images: parse("rustfs_images", 0),
        total_size: parse("rustfs_size", 0),
    };

    // Missing field invalidates the whole cache entry so an old-format
    // cache (written before total_quota existed) is repopulated.
    let total_quota = match stats_map.get("total_quota")? {
        v if v.is_empty() => None,
        v => v.parse::<i64>().ok(),
    };

    let mut backends = HashMap::new();
    backends.insert("local".to_string(), local);
    backends.insert("rustfs".to_string(), rustfs);

    TOTAL_USERS.set(total_users);
    TOTAL_IMAGES.set(total_images);
    TOTAL_STORAGE_BYTES.set(total_size);

    Some(AdminStats {
        total_users,
        total_images,
        total_size,
        active_users_24h,
        total_quota,
        storage_backends: backends,
    })
}

async fn query_total_users<DB: DbType>(pool: &Pool<DB>, locale: Language) -> Result<i64, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (i64,): crate::db::DbRow<DB>,
{
    sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin stats user count failed: {e}");
            internal_error(locale)
        })
}

async fn query_total_quota<DB: DbType>(
    pool: &Pool<DB>,
    locale: Language,
) -> Result<Option<i64>, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
{
    sqlx::query_scalar(
        "SELECT CAST(SUM(storage_quota) AS BIGINT) FROM users WHERE storage_quota IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin stats quota query failed: {e}");
        internal_error(locale)
    })
}

async fn query_image_stats<DB: DbType>(
    pool: &Pool<DB>,
    locale: Language,
) -> Result<(i64, i64), AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64, i64): crate::db::DbRow<DB>,
{
    sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT COUNT(*) as total_images, CAST(COALESCE(SUM(file_size), 0) AS BIGINT) as total_size
           FROM images"#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin stats image query failed: {e}");
        internal_error(locale)
    })
}

async fn query_active_users_24h<DB: DbType>(pool: &Pool<DB>) -> i64
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (i64,): crate::db::DbRow<DB>,
    chrono::DateTime<chrono::Utc>: sqlx::Type<DB> + for<'q> sqlx::Encode<'q, DB>,
{
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
    sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT user_id) FROM images
           WHERE created_at >= $1"#,
    )
    .bind(cutoff)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

async fn query_backend_stats<DB: DbType>(
    pool: &Pool<DB>,
    backend_name: &str,
    locale: Language,
) -> Result<BackendStats, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64, i64): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
{
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*), CAST(COALESCE(SUM(file_size), 0) AS BIGINT) FROM images WHERE storage_backend = $1",
    )
    .bind(backend_name)
    .fetch_one(pool)
    .await
    .map(|(total_images, total_size)| BackendStats {
        total_images,
        total_size,
    })
    .map_err(|e| {
        tracing::warn!("Admin stats backend query ({backend_name}) failed: {e}");
        internal_error(locale)
    })
}

async fn populate_stats_cache(cache: &dyn pichost_core::state::Cache, params: StatsCacheParams) {
    let StatsCacheParams {
        total_users,
        total_images,
        total_size,
        active_users_24h,
        total_quota,
        local_images,
        local_size,
        rustfs_size,
    } = params;
    // Overwrite (HSET) — the cache is repopulated from DB on every miss,
    // so accumulating with HINCRBY would double-count.
    let _ = cache
        .set_user_stats(
            &uuid::Uuid::nil(),
            &[
                ("total_users", Some(total_users)),
                ("total_images", Some(total_images)),
                ("total_size", Some(total_size)),
                ("active_users_24h", Some(active_users_24h)),
                ("total_quota", total_quota),
                ("local_images", Some(local_images)),
                ("local_size", Some(local_size)),
                ("rustfs_size", Some(rustfs_size)),
            ],
        )
        .await;
}

fn build_backends_map(
    local: &BackendStats,
    rustfs: &BackendStats,
) -> HashMap<String, BackendStats> {
    let mut m = HashMap::new();
    m.insert(
        "local".into(),
        BackendStats {
            total_images: local.total_images,
            total_size: local.total_size,
        },
    );
    m.insert(
        "rustfs".into(),
        BackendStats {
            total_images: rustfs.total_images,
            total_size: rustfs.total_size,
        },
    );
    m
}

// ── Config helpers ────────────────────────────────────────────────────

fn mask_url(url: &str) -> String {
    let re = regex::Regex::new(r"://([^:]*):([^@]*)@").unwrap();
    re.replace(url, "://$1:***@").to_string()
}

fn config_file_path() -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("config.toml")
}

// ── Handlers ───────────────────────────────────────────────────────────

/// GET /api/v1/admin/users — paginated user list (admin only)
pub async fn list_users<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ListUsersResponse>, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (i64,): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let offset = pagination.offset.unwrap_or(0).max(0);
    let limit = pagination.limit.unwrap_or(50).clamp(1, 200);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin user count query failed: {e}");
            internal_error(locale.0)
        })?;

    let rows = sqlx::query_as::<_, UserRow>(
        r#"SELECT id, username, email, is_admin, storage_backend, created_at, storage_quota, watermark_config
           FROM users ORDER BY created_at DESC OFFSET $1 LIMIT $2"#,
    )
    .bind(offset)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin user list query failed: {e}");
        internal_error(locale.0)
    })?;

    Ok(Json(ListUsersResponse {
        users: map_user_rows(rows),
        total,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserBody {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
    pub storage_backend: Option<String>,
    pub storage_quota: Option<i64>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_jsonb",
        skip_serializing_if = "Option::is_none"
    )]
    pub watermark_config: Option<Option<serde_json::Value>>,
}

/// PATCH /api/v1/admin/users/{id} — update user fields (admin only)
pub async fn update_user<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(current_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
    JsonBody(body): JsonBody<UpdateUserBody>,
) -> Result<Json<UserInfo>, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    // Prevent self-demotion
    if body.is_admin == Some(false) && current_user.id == user_id {
        return Err(error_json(
            locale.0,
            StatusCode::BAD_REQUEST,
            "admin.cannot_demote_self",
        ));
    }

    let (
        new_username,
        new_email,
        new_is_admin,
        new_storage_backend,
        new_storage_quota,
        new_watermark_config,
    ) = fetch_and_merge_user_fields(&state.pool, user_id, &body, locale.0).await?;

    let password_hash = hash_password_if_provided(&body.password, locale.0).await?;

    execute_user_update(
        UserUpdateParams::<'_, DB> {
            pool: &state.pool,
            user_id,
            username: &new_username,
            email: &new_email,
            is_admin: new_is_admin,
            storage_backend: &new_storage_backend,
            storage_quota: new_storage_quota,
            watermark_config: new_watermark_config,
            password_hash: password_hash.as_deref(),
        },
        locale.0,
    )
    .await?;

    tracing::info!(admin_id = %current_user.id, target_user = %user_id, "user updated");

    Ok(Json(UserInfo {
        id: user_id,
        username: new_username,
        email: new_email,
        is_admin: new_is_admin,
        storage_quota: new_storage_quota,
    }))
}

/// DELETE /api/v1/admin/users/{id} — delete user and all images (admin only)
pub async fn delete_user<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(current_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    (bool,): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    // Prevent self-deletion
    if current_user.id == user_id {
        return Err(error_json(
            locale.0,
            StatusCode::BAD_REQUEST,
            "admin.cannot_delete_self",
        ));
    }

    verify_user_exists(&state.pool, user_id, locale.0).await?;

    let images_deleted =
        collect_and_cleanup_storage_files(&state.router, &state.pool, user_id, locale.0).await?;

    // Delete from DB (cascade handles images)
    delete_user_from_db(&state.pool, user_id, locale.0).await?;

    tracing::info!(
        admin_id = %current_user.id,
        target_user = %user_id,
        images_deleted,
        "user deleted"
    );
    Ok((
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"message": "user deleted"})),
    ))
}

// ---- Admin Stats ----

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
    /// Sum of all users' storage_quota — None when no quotas are set.
    pub total_quota: Option<i64>,
    pub storage_backends: HashMap<String, BackendStats>,
}

/// GET /api/v1/admin/stats — system-wide statistics (admin only, cached 5 min)
pub async fn get_admin_stats<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
) -> Result<Json<AdminStats>, AdminError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64, i64): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    chrono::DateTime<chrono::Utc>: sqlx::Type<DB> + for<'q> sqlx::Encode<'q, DB>,
{
    // Try cache first using nil UUID as admin stats key
    if let Ok(Some(stats_map)) = state.cache.get_user_stats(&uuid::Uuid::nil()).await {
        if let Some(stats) = try_parse_cached_stats(&stats_map) {
            return Ok(Json(stats));
        }
    }

    // Cache miss — query DB
    let total_users = query_total_users(&state.pool, locale.0).await?;
    let (total_images, total_size) = query_image_stats(&state.pool, locale.0).await?;
    let active_users_24h = query_active_users_24h(&state.pool).await;
    let total_quota = query_total_quota(&state.pool, locale.0).await?;

    let local_stats = query_backend_stats(&state.pool, "local", locale.0).await?;
    let rustfs_stats = query_backend_stats(&state.pool, "rustfs", locale.0).await?;

    let stats = AdminStats {
        total_users,
        total_images,
        total_size,
        active_users_24h,
        total_quota,
        storage_backends: build_backends_map(&local_stats, &rustfs_stats),
    };

    // Populate cache (best-effort)
    populate_stats_cache(
        state.cache.as_ref(),
        StatsCacheParams {
            total_users,
            total_images,
            total_size,
            active_users_24h,
            total_quota,
            local_images: local_stats.total_images,
            local_size: local_stats.total_size,
            rustfs_size: rustfs_stats.total_size,
        },
    )
    .await;

    TOTAL_USERS.set(stats.total_users);
    TOTAL_IMAGES.set(stats.total_images);
    TOTAL_STORAGE_BYTES.set(stats.total_size);

    Ok(Json(stats))
}

// ── Invite Code handlers ──

/// POST /api/v1/admin/invites — create an invite code (admin only)
pub async fn create_invite<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(admin): Extension<AuthUser>,
    JsonBody(body): JsonBody<CreateInviteBody>,
) -> Result<Json<CreateInviteResponse>, AdminError> {
    let ttl_days = body.ttl_days.unwrap_or(7).clamp(1, 90);
    let ttl_secs = ttl_days * 86400;
    let now = chrono::Utc::now().timestamp();
    let expires_at = now + ttl_secs as i64;

    let code = Uuid::new_v4().to_string().replace('-', "");
    state
        .invites
        .create(&code, admin.id, ttl_secs)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to create invite code: {e}");
            internal_error(locale.0)
        })?;

    Ok(Json(CreateInviteResponse { code, expires_at }))
}

/// GET /api/v1/admin/invites — list all invite codes (admin only)
pub async fn list_invites<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
) -> Result<Json<Vec<InviteCodeInfo>>, AdminError> {
    let codes = state.invites.list().await.map_err(|e| {
        tracing::warn!("Failed to list invite codes: {e}");
        internal_error(locale.0)
    })?;
    Ok(Json(
        codes
            .into_iter()
            .map(|c| InviteCodeInfo {
                code: c.code,
                created_by: c.created_by.unwrap_or_default(),
                expires_at: c.expires_at.map(|t| t.timestamp()).unwrap_or(0),
                used_by: c.used_by,
                created_at: c.created_at.timestamp(),
            })
            .collect(),
    ))
}

// ── Config management handlers (P4-I) ─────────────────────────────────

/// GET /api/v1/admin/config — current config with sensitive fields masked
pub async fn get_admin_config(locale: Locale) -> Result<Json<ConfigResponse>, AdminError> {
    let path = config_file_path();
    let cfg = config::read_config_toml(&path).map_err(|e| {
        tracing::warn!("read config.toml failed: {e}");
        internal_error(locale.0)
    })?;

    Ok(Json(ConfigResponse {
        database_url: cfg
            .database_url
            .as_deref()
            .map(mask_url)
            .unwrap_or_else(|| "not set".into()),
        redis_url: cfg
            .redis_url
            .as_deref()
            .map(mask_url)
            .unwrap_or_else(|| "not set".into()),
        jwt_secret: "********".into(),
        token_encryption_key: if cfg.token_encryption_key.is_some() {
            "********".into()
        } else {
            "not set".into()
        },
        public_url: cfg.public_url.unwrap_or_else(|| "not set".into()),
        default_backend: cfg.default_backend.unwrap_or_else(|| "local".into()),
        local_base_path: cfg
            .local_base_path
            .unwrap_or_else(|| "./storage-local".into()),
        config_path: path.display().to_string(),
        i18n: ConfigResponseI18n {
            language: cfg.i18n_language.unwrap_or_else(|| "en".into()),
            locales_dir: cfg.i18n_locales_dir.unwrap_or_else(|| "not set".into()),
        },
    }))
}

/// PUT /api/v1/admin/config — write config.toml with auto-backup.
/// Merge semantics: fields omitted from the body (None) keep their
/// current on-disk value, so a partial update never wipes other keys.
pub async fn update_admin_config<DB: DbType>(
    state: State<Arc<AppState<DB>>>,
    locale: Locale,
    JsonBody(body): JsonBody<UpdateConfigBody>,
) -> Result<Json<ConfigResponse>, AdminError> {
    let path = config_file_path();
    // Best-effort backup — first save may have no existing config.toml
    let _ = config::backup_config(&path);

    let existing = config::read_config_toml(&path).unwrap_or_default();
    // i18n 键回退链: body → config.toml → 运行时生效配置(env),避免覆盖 env 部署的默认值
    let effective_language = state.config.i18n.language.clone();
    let effective_locales_dir = state
        .config
        .i18n
        .locales_dir
        .as_ref()
        .map(|p| p.display().to_string());
    let cfg = SystemConfig {
        database_url: body.database_url.or(existing.database_url),
        redis_url: body.redis_url.or(existing.redis_url),
        jwt_secret: None,
        token_encryption_key: None,
        public_url: body.public_url.or(existing.public_url),
        default_backend: body.default_backend.or(existing.default_backend),
        local_base_path: body.local_base_path.or(existing.local_base_path),
        i18n_language: body
            .i18n
            .as_ref()
            .and_then(|i| i.language.clone())
            .or(existing.i18n_language)
            .or(Some(effective_language)),
        i18n_locales_dir: body
            .i18n
            .as_ref()
            .and_then(|i| i.locales_dir.clone())
            .or(existing.i18n_locales_dir)
            .or(effective_locales_dir),
    };
    config::write_config_toml(&path, &cfg).map_err(|e| {
        error_json_args(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "config.write_failed",
            &[e.to_string()],
        )
    })?;

    // 语言/消息目录变更即时生效: 重新装载全局 i18n,无需重启
    I18n::reload_global(
        Language::from_str_opt(cfg.i18n_language.as_deref().unwrap_or("en")),
        cfg.i18n_locales_dir.as_ref().map(std::path::PathBuf::from),
    );

    get_admin_config(locale).await
}

/// POST /api/v1/admin/config/test — test DB/Redis connections
pub async fn test_admin_config(
    JsonBody(body): JsonBody<TestConfigBody>,
) -> Result<Json<TestResult>, AdminError> {
    let mut result = TestResult {
        database: None,
        redis: None,
    };
    if let Some(ref url) = body.database_url {
        result.database = Some(match config::test_database_connection(url).await {
            Ok(()) => "ok".into(),
            Err(e) => format!("fail: {e}"),
        });
    }
    if let Some(ref url) = body.redis_url {
        result.redis = Some(match config::test_redis_connection(url) {
            Ok(()) => "ok".into(),
            Err(e) => format!("fail: {e}"),
        });
    }
    Ok(Json(result))
}

/// POST /api/v1/admin/config/backup — materializes config.toml if missing, then backs it up
pub async fn backup_admin_config<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
) -> Result<Json<BackupInfo>, AdminError> {
    let path = config_file_path();
    if !path.exists() {
        let cfg = SystemConfig::from_effective(&state.config);
        config::write_config_toml(&path, &cfg).map_err(|e| {
            error_json_args(
                locale.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "config.materialize_failed",
                &[e.to_string()],
            )
        })?;
    }
    let filename = config::backup_config(&path).map_err(|e| {
        tracing::warn!("backup config failed: {e}");
        internal_error(locale.0)
    })?;
    Ok(Json(BackupInfo { filename }))
}

/// GET /api/v1/admin/config/backups — list backup files, newest first
pub async fn list_config_backups(locale: Locale) -> Result<Json<Vec<BackupInfo>>, AdminError> {
    let dir = std::env::current_dir().unwrap_or_default();
    let backups = config::list_backups(&dir)
        .map_err(|e| {
            tracing::warn!("list config backups failed: {e}");
            internal_error(locale.0)
        })?
        .into_iter()
        .map(|filename| BackupInfo { filename })
        .collect();
    Ok(Json(backups))
}

/// POST /api/v1/admin/config/restore — restore config.toml from a backup
pub async fn restore_admin_config(
    locale: Locale,
    JsonBody(body): JsonBody<RestoreBackupBody>,
) -> Result<Json<serde_json::Value>, AdminError> {
    let path = config_file_path();
    config::restore_config(&path, &body.backup_file).map_err(|e| match e {
        config::ConfigError::Io(m) if m.starts_with("Backup not found") => error_json_args(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "config.backup_not_found",
            &[m],
        ),
        e => {
            tracing::warn!("restore config failed: {e}");
            internal_error(locale.0)
        }
    })?;
    // 恢复的 config.toml 可能携带不同 i18n 配置 — 立即重载,与 PUT 路径一致
    let restored = config::read_config_toml(&path).unwrap_or_default();
    I18n::reload_global(
        Language::from_str_opt(restored.i18n_language.as_deref().unwrap_or("en")),
        restored
            .i18n_locales_dir
            .as_ref()
            .map(std::path::PathBuf::from),
    );
    Ok(Json(serde_json::json!({
        "status": "restored",
        "from": body.backup_file,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn user_row() -> UserRow {
        (
            Uuid::new_v4(),
            "alice".into(),
            Some("a@b.c".into()),
            true,
            "local".into(),
            chrono::Utc::now(),
            Some(1024),
            None,
        )
    }

    #[test]
    fn test_map_user_rows() {
        let infos = map_user_rows(vec![user_row()]);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].username, "alice");
        assert_eq!(infos[0].email.as_deref(), Some("a@b.c"));
        assert_eq!(infos[0].storage_quota, Some(1024));
        assert!(infos[0].is_admin);
    }

    fn empty_body() -> UpdateUserBody {
        UpdateUserBody {
            username: None,
            email: None,
            password: None,
            is_admin: None,
            storage_backend: None,
            storage_quota: None,
            watermark_config: None,
        }
    }

    #[test]
    fn test_merge_user_fields_all_none_keeps_existing() {
        let existing = (
            "bob".into(),
            Some("b@c.d".into()),
            false,
            "local".into(),
            Some(5),
            None,
        );
        let merged = merge_user_fields(&empty_body(), existing);
        assert_eq!(merged.0, "bob");
        assert_eq!(merged.1, Some("b@c.d".into()));
        assert!(!merged.2);
        assert_eq!(merged.4, Some(5));
    }

    #[test]
    fn test_merge_user_fields_updates() {
        let existing = ("bob".into(), None, false, "local".into(), Some(5), None);
        let mut body = empty_body();
        body.username = Some("alice".into());
        body.is_admin = Some(true);
        body.storage_quota = Some(10);
        let merged = merge_user_fields(&body, existing);
        assert_eq!(merged.0, "alice");
        assert!(merged.2);
        assert_eq!(merged.4, Some(10));
    }

    #[test]
    fn test_merge_user_fields_quota_zero_clears() {
        let existing = ("bob".into(), None, false, "local".into(), Some(5), None);
        let mut body = empty_body();
        body.storage_quota = Some(0);
        let merged = merge_user_fields(&body, existing);
        assert_eq!(merged.4, None);
    }

    #[test]
    fn test_merge_user_fields_watermark_null_clears() {
        let existing = (
            "bob".into(),
            None,
            false,
            "local".into(),
            None,
            Some(serde_json::json!({"enabled": true})),
        );
        let mut body = empty_body();
        body.watermark_config = Some(None);
        let merged = merge_user_fields(&body, existing);
        assert_eq!(merged.5, None);
    }

    #[test]
    fn test_merge_user_fields_watermark_absent_keeps() {
        let existing = (
            "bob".into(),
            None,
            false,
            "local".into(),
            None,
            Some(serde_json::json!({"enabled": true})),
        );
        let merged = merge_user_fields(&empty_body(), existing);
        assert!(merged.5.is_some());
    }

    fn stats_map() -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (k, v) in [
            ("total_users", "10"),
            ("total_images", "20"),
            ("total_size", "30"),
            ("active_users_24h", "2"),
            ("total_quota", "100"),
            ("local_images", "15"),
            ("local_size", "25"),
            ("rustfs_size", "5"),
        ] {
            map.insert(k.to_string(), v.to_string());
        }
        map
    }

    #[test]
    fn test_try_parse_cached_stats_complete() {
        let stats = try_parse_cached_stats(&stats_map()).unwrap();
        assert_eq!(stats.total_users, 10);
        assert_eq!(stats.total_images, 20);
        assert_eq!(stats.total_size, 30);
        assert_eq!(stats.active_users_24h, 2);
        assert_eq!(stats.total_quota, Some(100));
        assert_eq!(stats.storage_backends.len(), 2);
        assert_eq!(stats.storage_backends["local"].total_images, 15);
    }

    #[test]
    fn test_try_parse_cached_stats_missing_quota() {
        let mut map = stats_map();
        map.remove("total_quota");
        assert!(try_parse_cached_stats(&map).is_none());
    }

    #[test]
    fn test_try_parse_cached_stats_unparseable() {
        let mut map = stats_map();
        map.insert("total_users".into(), "abc".into());
        assert!(try_parse_cached_stats(&map).is_none());
    }

    #[test]
    fn test_build_backends_map() {
        let local = BackendStats {
            total_images: 1,
            total_size: 2,
        };
        let rustfs = BackendStats {
            total_images: 3,
            total_size: 4,
        };
        let m = build_backends_map(&local, &rustfs);
        assert_eq!(m.len(), 2);
        assert_eq!(m["local"].total_images, 1);
        assert_eq!(m["local"].total_size, 2);
        assert_eq!(m["rustfs"].total_images, 3);
        assert_eq!(m["rustfs"].total_size, 4);
    }

    #[test]
    fn test_mask_url() {
        assert_eq!(
            mask_url("postgres://user:pass@host:5432/db"),
            "postgres://user:***@host:5432/db"
        );
        assert_eq!(mask_url("redis://:secret@r:6379"), "redis://:***@r:6379");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_hash_password_if_provided() {
        assert_eq!(
            hash_password_if_provided(&None, Language::En)
                .await
                .unwrap(),
            None
        );
        let err = hash_password_if_provided(&Some("short".into()), Language::En)
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        let hash = hash_password_if_provided(&Some("long-enough-pass".into()), Language::En)
            .await
            .unwrap()
            .unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    #[derive(Deserialize)]
    struct WmTestBody {
        #[serde(default, deserialize_with = "deserialize_optional_jsonb")]
        watermark_config: Option<Option<serde_json::Value>>,
    }

    #[test]
    fn test_deserialize_optional_jsonb() {
        let b: WmTestBody = serde_json::from_str(r#"{"watermark_config": null}"#).unwrap();
        assert_eq!(b.watermark_config, Some(None));
        let b: WmTestBody = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(b.watermark_config, None);
        let b: WmTestBody =
            serde_json::from_str(r#"{"watermark_config": {"enabled": true}}"#).unwrap();
        assert!(b.watermark_config.unwrap().is_some());
    }

    #[test]
    fn test_config_file_path() {
        let path = config_file_path();
        assert_eq!(path.file_name().unwrap(), "config.toml");
    }
}
