use std::sync::Arc;

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    Extension,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use pichost_core::{
    crypto::{decode_key, encrypt_token, mask_token},
    error::AppError,
    models::{UserStorageConfig, UserStorageConfigResponse},
};

use crate::{app::AppState, middleware::auth::AuthUser};

// ── Request types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateConfigRequest {
    pub name: String,
    pub provider: String,
    pub token: String,
    pub repo: String,
    pub branch: Option<String>,
    pub path_prefix: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub name: Option<String>,
    pub token: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub path_prefix: Option<String>,
}

// ── Private helpers ─────────────────────────────────────────────────────

fn build_response(config: &UserStorageConfig) -> UserStorageConfigResponse {
    let detail = &config.config;
    let repo = detail["repo"].as_str().unwrap_or("").to_string();
    let branch = detail["branch"].as_str().unwrap_or("main").to_string();
    let path_prefix = detail["path_prefix"].as_str().map(|s| s.to_string());
    let token = detail["token_encrypted"].as_str().unwrap_or("");
    let masked = mask_token(token);

    UserStorageConfigResponse {
        id: config.id,
        name: config.name.clone(),
        provider: config.provider.clone(),
        repo,
        branch,
        path_prefix,
        is_default: config.is_default,
        token_masked: masked,
        created_at: config.created_at,
        updated_at: config.updated_at,
    }
}

async fn check_config_limit(pool: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_storage_configs WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    if count >= 5 {
        return Err(AppError::bad_request("最多只能创建5个存储配置"));
    }
    Ok(())
}

async fn check_name_unique(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
) -> Result<(), AppError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_storage_configs \
         WHERE user_id = $1 AND name = $2)",
    )
    .bind(user_id)
    .bind(name)
    .fetch_one(pool)
    .await?;

    if exists {
        return Err(AppError::bad_request("配置名称已存在"));
    }
    Ok(())
}

async fn unset_other_defaults(pool: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE user_storage_configs SET is_default = false WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn fetch_user_config(
    pool: &PgPool,
    config_id: Uuid,
    user_id: Uuid,
) -> Result<UserStorageConfig, AppError> {
    sqlx::query_as::<_, UserStorageConfig>(
        "SELECT id, user_id, name, provider, is_default, \
         config, created_at, updated_at \
         FROM user_storage_configs WHERE id = $1 AND user_id = $2",
    )
    .bind(config_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("存储配置不存在"))
}

fn encrypt_token_from_config(
    token: &str,
    encryption_key: &str,
) -> Result<String, AppError> {
    let key_bytes = decode_key(encryption_key)?;
    Ok(encrypt_token(token, &key_bytes)?)
}

fn build_config_json(req: &CreateConfigRequest, encrypted: String) -> serde_json::Value {
    let branch = req.branch.clone().unwrap_or_else(|| "main".to_string());
    serde_json::json!({
        "token_encrypted": encrypted,
        "repo": req.repo,
        "branch": branch,
        "path_prefix": req.path_prefix,
    })
}

fn merge_config_detail(
    mut detail: serde_json::Value,
    req: &UpdateConfigRequest,
    encrypted_token: Option<String>,
) -> serde_json::Value {
    if let Some(repo) = &req.repo {
        detail["repo"] = serde_json::Value::String(repo.clone());
    }
    if let Some(branch) = &req.branch {
        detail["branch"] = serde_json::Value::String(branch.clone());
    }
    if let Some(ref path_prefix) = req.path_prefix {
        detail["path_prefix"] = serde_json::Value::String(path_prefix.clone());
    }
    if let Some(encrypted) = encrypted_token {
        detail["token_encrypted"] = serde_json::Value::String(encrypted);
    }
    detail
}

// ── Handlers ────────────────────────────────────────────────────────────

/// GET /api/v1/users/me/storage-configs
pub async fn list_configs(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<UserStorageConfigResponse>>, AppError> {
    let configs = sqlx::query_as::<_, UserStorageConfig>(
        "SELECT id, user_id, name, provider, is_default, \
         config, created_at, updated_at \
         FROM user_storage_configs WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let responses: Vec<UserStorageConfigResponse> =
        configs.iter().map(build_response).collect();

    Ok(Json(responses))
}

/// POST /api/v1/users/me/storage-configs
pub async fn create_config(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateConfigRequest>,
) -> Result<(StatusCode, Json<UserStorageConfigResponse>), AppError> {
    if !["github", "gitcode"].contains(&req.provider.as_str()) {
        return Err(AppError::bad_request(
            "不支持的存储类型，仅支持 github 和 gitcode",
        ));
    }

    check_config_limit(&state.pool, user.id).await?;
    check_name_unique(&state.pool, user.id, &req.name).await?;

    let encryption_key = state
        .config
        .token_encryption_key
        .as_ref()
        .ok_or_else(|| AppError::internal("系统未配置加密密钥"))?;

    let encrypted = encrypt_token_from_config(&req.token, encryption_key)?;
    let config_json = build_config_json(&req, encrypted);
    let is_default = req.is_default.unwrap_or(false);

    if is_default {
        unset_other_defaults(&state.pool, user.id).await?;
    }

    let config = sqlx::query_as::<_, UserStorageConfig>(
        "INSERT INTO user_storage_configs \
         (user_id, name, provider, is_default, config) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, user_id, name, provider, is_default, \
                   config, created_at, updated_at",
    )
    .bind(user.id)
    .bind(&req.name)
    .bind(&req.provider)
    .bind(is_default)
    .bind(&config_json)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(build_response(&config))))
}

/// GET /api/v1/users/me/storage-configs/{id}
pub async fn get_config(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserStorageConfigResponse>, AppError> {
    let config = fetch_user_config(&state.pool, id, user.id).await?;
    Ok(Json(build_response(&config)))
}

/// PATCH /api/v1/users/me/storage-configs/{id}
pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<UserStorageConfigResponse>, AppError> {
    let existing = fetch_user_config(&state.pool, id, user.id).await?;

    let new_name = req.name.as_ref().cloned()
        .unwrap_or_else(|| existing.name.clone());
    if new_name != existing.name {
        check_name_unique(&state.pool, user.id, &new_name).await?;
    }

    let encrypted_token = if let Some(token) = &req.token {
        let encryption_key = state
            .config
            .token_encryption_key
            .as_ref()
            .ok_or_else(|| AppError::internal("系统未配置加密密钥"))?;
        Some(encrypt_token_from_config(token, encryption_key)?)
    } else {
        None
    };

    let detail = merge_config_detail(existing.config.clone(), &req, encrypted_token);

    let updated = sqlx::query_as::<_, UserStorageConfig>(
        "UPDATE user_storage_configs SET name = $1, config = $2, \
         updated_at = now() \
         WHERE id = $3 AND user_id = $4 \
         RETURNING id, user_id, name, provider, is_default, \
                   config, created_at, updated_at",
    )
    .bind(&new_name)
    .bind(&detail)
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(build_response(&updated)))
}

/// DELETE /api/v1/users/me/storage-configs/{id}
pub async fn delete_config(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let _existing = fetch_user_config(&state.pool, id, user.id).await?;

    let ref_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM images \
         WHERE storage_config_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    if ref_count > 0 {
        return Err(AppError::bad_request("该存储配置下还有图片，请先删除相关图片"));
    }

    sqlx::query("DELETE FROM user_storage_configs WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/users/me/storage-configs/{id}/default
pub async fn set_default(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserStorageConfigResponse>, AppError> {
    let _existing = fetch_user_config(&state.pool, id, user.id).await?;

    unset_other_defaults(&state.pool, user.id).await?;

    let config = sqlx::query_as::<_, UserStorageConfig>(
        "UPDATE user_storage_configs SET is_default = true, updated_at = now() \
         WHERE id = $1 AND user_id = $2 \
         RETURNING id, user_id, name, provider, is_default, \
                   config, created_at, updated_at",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(build_response(&config)))
}
