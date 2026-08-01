use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use pichost_core::StorageRouter;

fn deserialize_optional_jsonb<'de, D>(deserializer: D) -> Result<Option<Option<serde_json::Value>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

use crate::app::AppState;
use crate::cache::{Cache, InviteCodeInfo};
use crate::db::DbPool;
use crate::metrics::{TOTAL_IMAGES, TOTAL_STORAGE_BYTES, TOTAL_USERS};
use crate::middleware::auth::AuthUser;
use crate::routes::auth::UserInfo;
use crate::services::config::{self, SystemConfig};

// ── Error shorthand ──

type AdminError = (StatusCode, Json<serde_json::Value>);

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

struct UserUpdateParams<'a> {
    pool: &'a DbPool,
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
            |(id, username, email, _is_admin, _storage_backend, _created_at, storage_quota, _watermark_config)| UserInfo {
                id,
                username,
                email,
                is_admin: _is_admin,
                storage_quota,
            },
        )
        .collect()
}

// ── update_user helpers ────────────────────────────────────────────────

async fn hash_password_if_provided(
    password: &Option<String>,
) -> Result<Option<String>, AdminError> {
    let Some(password) = password else {
        return Ok(None);
    };
    if password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "password must be at least 8 characters"})),
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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?
        .to_string();
    Ok(Some(hash))
}

async fn execute_user_update(params: UserUpdateParams<'_>) -> Result<(), AdminError> {
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
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
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
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;
    }
    Ok(())
}

async fn fetch_and_merge_user_fields(
    pool: &DbPool,
    user_id: Uuid,
    body: &UpdateUserBody,
) -> Result<(String, Option<String>, bool, String, Option<i64>, Option<serde_json::Value>), AdminError> {
    let existing = sqlx::query_as::<_, (String, Option<String>, bool, String, Option<i64>, Option<serde_json::Value>)>(
        r#"SELECT username, email, is_admin, storage_backend, storage_quota, watermark_config FROM users WHERE id = $1"#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin update user query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        )
    })?;

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

    Ok((new_username, new_email, new_is_admin, new_storage_backend, new_storage_quota, new_watermark_config))
}

// ── delete_user helpers ────────────────────────────────────────────────

async fn collect_and_cleanup_storage_files(
    router: &StorageRouter,
    pool: &DbPool,
    user_id: Uuid,
) -> Result<usize, AdminError> {
    let image_keys: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT storage_key, thumbnail_key, webp_key FROM images WHERE user_id = $1"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin delete user image keys query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
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

async fn delete_user_from_db(pool: &DbPool, user_id: Uuid) -> Result<(), AdminError> {
    sqlx::query("DELETE FROM images WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin delete user images failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin delete user failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    Ok(())
}

async fn verify_user_exists(pool: &DbPool, user_id: Uuid) -> Result<(), AdminError> {
    let exists: bool =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                tracing::warn!("Admin delete user check failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal error"})),
                )
            })?;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        ));
    }
    Ok(())
}

// ── get_admin_stats helpers ────────────────────────────────────────────

fn try_parse_cached_stats(stats_map: &HashMap<String, String>) -> Option<AdminStats> {
    let total_users: i64 = stats_map.get("total_users")?.parse().ok()?;
    let total_images: i64 = stats_map.get("total_images")?.parse().ok()?;
    let total_size: i64 = stats_map.get("total_size")?.parse().ok()?;
    let active_users_24h: i64 = stats_map.get("active_users_24h")
        .and_then(|v| v.parse().ok()).unwrap_or(0);

    let parse = |k: &str, fallback| stats_map.get(k).and_then(|v| v.parse().ok()).unwrap_or(fallback);
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

    Some(AdminStats { total_users, total_images, total_size, active_users_24h, total_quota, storage_backends: backends })
}

async fn query_total_users(pool: &DbPool) -> Result<i64, AdminError> {
    sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin stats user count failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })
}

async fn query_total_quota(pool: &DbPool) -> Result<Option<i64>, AdminError> {
    sqlx::query_scalar(
        "SELECT SUM(storage_quota)::BIGINT FROM users WHERE storage_quota IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin stats quota query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })
}

async fn query_image_stats(pool: &DbPool) -> Result<(i64, i64), AdminError> {
    sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT COUNT(*)::BIGINT as total_images, COALESCE(SUM(file_size), 0)::BIGINT as total_size
           FROM images"#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin stats image query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })
}

async fn query_active_users_24h(pool: &DbPool) -> i64 {
    sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT user_id) FROM images
           WHERE created_at > NOW() - INTERVAL '24 hours'"#,
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

async fn query_backend_stats(
    pool: &DbPool,
    backend_name: &str,
) -> Result<BackendStats, AdminError> {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*)::BIGINT, COALESCE(SUM(file_size), 0)::BIGINT FROM images WHERE storage_backend = $1",
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })
}

async fn populate_stats_cache(cache: &Cache, params: StatsCacheParams) {
    let StatsCacheParams {
        total_users, total_images, total_size, active_users_24h, total_quota,
        local_images, local_size, rustfs_size,
    } = params;
    // Overwrite (HSET) — the cache is repopulated from DB on every miss,
    // so accumulating with HINCRBY would double-count.
    let _ = cache
        .set_user_stats(&uuid::Uuid::nil(), &[
            ("total_users", Some(total_users)),
            ("total_images", Some(total_images)),
            ("total_size", Some(total_size)),
            ("active_users_24h", Some(active_users_24h)),
            ("total_quota", total_quota),
            ("local_images", Some(local_images)),
            ("local_size", Some(local_size)),
            ("rustfs_size", Some(rustfs_size)),
        ])
        .await;
}

fn build_backends_map(local: &BackendStats, rustfs: &BackendStats) -> HashMap<String, BackendStats> {
    let mut m = HashMap::new();
    m.insert("local".into(), BackendStats {
        total_images: local.total_images,
        total_size: local.total_size,
    });
    m.insert("rustfs".into(), BackendStats {
        total_images: rustfs.total_images,
        total_size: rustfs.total_size,
    });
    m
}

// ── Config helpers ────────────────────────────────────────────────────

fn mask_url(url: &str) -> String {
    let re = regex::Regex::new(r"://([^:]*):([^@]*)@").unwrap();
    re.replace(url, "://$1:***@").to_string()
}

fn config_file_path() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_default().join("config.toml")
}

fn internal_error(msg: String) -> AdminError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": msg})),
    )
}

// ── Handlers ───────────────────────────────────────────────────────────

/// GET /api/v1/admin/users — paginated user list (admin only)
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ListUsersResponse>, (StatusCode, Json<serde_json::Value>)> {
    let offset = pagination.offset.unwrap_or(0).max(0);
    let limit = pagination.limit.unwrap_or(50).clamp(1, 200);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin user count query failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
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
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<UpdateUserBody>,
) -> Result<Json<UserInfo>, (StatusCode, Json<serde_json::Value>)> {
    // Prevent self-demotion
    if body.is_admin == Some(false) && current_user.id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cannot demote yourself"})),
        ));
    }

    let (new_username, new_email, new_is_admin, new_storage_backend, new_storage_quota, new_watermark_config) =
        fetch_and_merge_user_fields(&state.pool, user_id, &body).await?;

    let password_hash = hash_password_if_provided(&body.password).await?;

    execute_user_update(UserUpdateParams {
        pool: &state.pool,
        user_id,
        username: &new_username,
        email: &new_email,
        is_admin: new_is_admin,
        storage_backend: &new_storage_backend,
        storage_quota: new_storage_quota,
        watermark_config: new_watermark_config,
        password_hash: password_hash.as_deref(),
    })
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
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    // Prevent self-deletion
    if current_user.id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cannot delete yourself"})),
        ));
    }

    verify_user_exists(&state.pool, user_id).await?;

    let images_deleted = collect_and_cleanup_storage_files(&state.router, &state.pool, user_id).await?;

    // Delete from DB (cascade handles images)
    delete_user_from_db(&state.pool, user_id).await?;

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
pub async fn get_admin_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminStats>, (StatusCode, Json<serde_json::Value>)> {
    // Try cache first using nil UUID as admin stats key
    if let Ok(Some(stats_map)) = state.cache.get_user_stats(&uuid::Uuid::nil()).await {
        if let Some(stats) = try_parse_cached_stats(&stats_map) {
            return Ok(Json(stats));
        }
    }

    // Cache miss — query DB
    let total_users = query_total_users(&state.pool).await?;
    let (total_images, total_size) = query_image_stats(&state.pool).await?;
    let active_users_24h = query_active_users_24h(&state.pool).await;
    let total_quota = query_total_quota(&state.pool).await?;

    let local_stats = query_backend_stats(&state.pool, "local").await?;
    let rustfs_stats = query_backend_stats(&state.pool, "rustfs").await?;

    let stats = AdminStats {
        total_users,
        total_images,
        total_size,
        active_users_24h,
        total_quota,
        storage_backends: build_backends_map(&local_stats, &rustfs_stats),
    };

    // Populate cache (best-effort)
    populate_stats_cache(&state.cache, StatsCacheParams {
        total_users, total_images, total_size, active_users_24h, total_quota,
        local_images: local_stats.total_images,
        local_size: local_stats.total_size,
        rustfs_size: rustfs_stats.total_size,
    }).await;

    TOTAL_USERS.set(stats.total_users);
    TOTAL_IMAGES.set(stats.total_images);
    TOTAL_STORAGE_BYTES.set(stats.total_size);

    Ok(Json(stats))
}

// ── Invite Code handlers ──

/// POST /api/v1/admin/invites — create an invite code (admin only)
pub async fn create_invite(
    State(state): State<Arc<AppState>>,
    Extension(admin): Extension<AuthUser>,
    Json(body): Json<CreateInviteBody>,
) -> Result<Json<CreateInviteResponse>, (StatusCode, Json<serde_json::Value>)> {
    let ttl_days = body.ttl_days.unwrap_or(7).clamp(1, 90);
    let ttl_secs = ttl_days * 86400;
    let now = chrono::Utc::now().timestamp();
    let expires_at = now + ttl_secs as i64;

    let code = state
        .cache
        .create_invite_code(&admin.id, ttl_secs)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to create invite code: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    Ok(Json(CreateInviteResponse { code, expires_at }))
}

/// GET /api/v1/admin/invites — list all invite codes (admin only)
pub async fn list_invites(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InviteCodeInfo>>, (StatusCode, Json<serde_json::Value>)> {
    let codes = state.cache.list_invite_codes().await.map_err(|e| {
        tracing::warn!("Failed to list invite codes: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?;
    Ok(Json(codes))
}

// ── Config management handlers (P4-I) ─────────────────────────────────

/// GET /api/v1/admin/config — current config with sensitive fields masked
pub async fn get_admin_config() -> Result<Json<ConfigResponse>, AdminError> {
    let path = config_file_path();
    let cfg = config::read_config_toml(&path).map_err(|e| internal_error(e.to_string()))?;

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
        local_base_path: cfg.local_base_path.unwrap_or_else(|| "./storage-local".into()),
        config_path: path.display().to_string(),
    }))
}

/// PUT /api/v1/admin/config — write config.toml with auto-backup
pub async fn update_admin_config(
    Json(body): Json<UpdateConfigBody>,
) -> Result<Json<ConfigResponse>, AdminError> {
    let path = config_file_path();
    // Best-effort backup — first save may have no existing config.toml
    let _ = config::backup_config(&path);

    let cfg = SystemConfig {
        database_url: body.database_url,
        redis_url: body.redis_url,
        jwt_secret: None,
        token_encryption_key: None,
        public_url: body.public_url,
        default_backend: body.default_backend,
        local_base_path: body.local_base_path,
    };
    config::write_config_toml(&path, &cfg)
        .map_err(|e| internal_error(format!("write failed: {e}")))?;

    get_admin_config().await
}

/// POST /api/v1/admin/config/test — test DB/Redis connections
pub async fn test_admin_config(
    Json(body): Json<TestConfigBody>,
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
pub async fn backup_admin_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BackupInfo>, AdminError> {
    let path = config_file_path();
    if !path.exists() {
        let cfg = SystemConfig::from_effective(&state.config);
        config::write_config_toml(&path, &cfg)
            .map_err(|e| internal_error(format!("materialize config failed: {e}")))?;
    }
    let filename = config::backup_config(&path).map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(BackupInfo { filename }))
}

/// GET /api/v1/admin/config/backups — list backup files, newest first
pub async fn list_config_backups() -> Result<Json<Vec<BackupInfo>>, AdminError> {
    let dir = std::env::current_dir().unwrap_or_default();
    let backups = config::list_backups(&dir)
        .map_err(|e| internal_error(e.to_string()))?
        .into_iter()
        .map(|filename| BackupInfo { filename })
        .collect();
    Ok(Json(backups))
}

/// POST /api/v1/admin/config/restore — restore config.toml from a backup
pub async fn restore_admin_config(
    Json(body): Json<RestoreBackupBody>,
) -> Result<Json<serde_json::Value>, AdminError> {
    let path = config_file_path();
    config::restore_config(&path, &body.backup_file).map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "status": "restored",
        "from": body.backup_file,
    })))
}
