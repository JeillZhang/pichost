use pichost_core::DbType;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use sqlx::Pool;
use uuid::Uuid;

use pichost_core::{
    crypto::{decode_key, encrypt_token, mask_token},
    error::AppError,
    i18n::Language,
    models::{UserStorageConfig, UserStorageConfigResponse},
};

use crate::{
    app::AppState,
    i18n_ext::{error_json, error_json_args, JsonBody, Locale},
    middleware::auth::AuthUser,
};

type ApiError = (StatusCode, Json<serde_json::Value>);

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

async fn check_config_limit<DB: DbType>(
    max_configs: Option<u32>,
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<(), ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (i64,): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let max = max_configs.unwrap_or(5) as i64;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_storage_configs WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                tracing::warn!("storage config count query failed: {e}");
                error_json(
                    locale,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "common.internal_error",
                )
            })?;

    if count >= max {
        return Err(error_json_args(
            locale,
            StatusCode::BAD_REQUEST,
            "storage_config.limit",
            &[max.to_string()],
        ));
    }
    Ok(())
}

async fn check_name_unique<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    name: &str,
    locale: Language,
) -> Result<(), ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (bool,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_storage_configs \
         WHERE user_id = $1 AND name = $2)",
    )
    .bind(user_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("storage config name check failed: {e}");
        error_json(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?;

    if exists {
        return Err(error_json(
            locale,
            StatusCode::CONFLICT,
            "storage_config.name_exists",
        ));
    }
    Ok(())
}

async fn unset_other_defaults<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    locale: Language,
) -> Result<(), ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query("UPDATE user_storage_configs SET is_default = false WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::warn!("unset storage config defaults failed: {e}");
            error_json(
                locale,
                StatusCode::INTERNAL_SERVER_ERROR,
                "common.internal_error",
            )
        })?;
    Ok(())
}

async fn fetch_user_config<DB: DbType>(
    pool: &Pool<DB>,
    config_id: Uuid,
    user_id: Uuid,
    locale: Language,
) -> Result<UserStorageConfig, ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_as::<_, UserStorageConfig>(
        "SELECT id, user_id, name, provider, is_default, \
         config, created_at, updated_at \
         FROM user_storage_configs WHERE id = $1 AND user_id = $2",
    )
    .bind(config_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("storage config fetch failed: {e}");
        error_json(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "storage_config.not_found"))
}

fn encrypt_token_from_config(token: &str, encryption_key: &str) -> Result<String, AppError> {
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

fn api_base_for_provider(provider: &str) -> Option<&str> {
    match provider {
        "github" => Some("https://api.github.com"),
        "gitcode" => Some("https://api.gitcode.com/api/v5"),
        _ => None,
    }
}

async fn verify_repo_access(
    provider: &str,
    token: &str,
    repo: &str,
    locale: Language,
) -> Result<(), ApiError> {
    let api_base = api_base_for_provider(provider).ok_or_else(|| {
        error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "storage_config.unsupported_type",
        )
    })?;
    let url = format!("{}/repos/{}", api_base, repo);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "pichost/0.15.0")
        .send()
        .await
        .map_err(|e| {
            error_json_args(
                locale,
                StatusCode::BAD_REQUEST,
                "storage_config.repo_unreachable",
                &[e.to_string()],
            )
        })?;

    match resp.status().as_u16() {
        200 | 302 => Ok(()),
        401 | 403 => Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "storage_config.token_invalid",
        )),
        404 => Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "storage_config.repo_not_found",
        )),
        code => {
            let body = resp.text().await.unwrap_or_default();
            Err(error_json_args(
                locale,
                StatusCode::BAD_REQUEST,
                "storage_config.repo_verify_failed",
                &[code.to_string(), body],
            ))
        }
    }
}

// ── Handlers ────────────────────────────────────────────────────────────

/// GET /api/v1/users/me/storage-configs
pub async fn list_configs<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<UserStorageConfigResponse>>, ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let configs = sqlx::query_as::<_, UserStorageConfig>(
        "SELECT id, user_id, name, provider, is_default, \
         config, created_at, updated_at \
         FROM user_storage_configs WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("storage config list failed: {e}");
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?;

    let responses: Vec<UserStorageConfigResponse> = configs.iter().map(build_response).collect();

    Ok(Json(responses))
}

/// POST /api/v1/users/me/storage-configs
pub async fn create_config<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(user): Extension<AuthUser>,
    JsonBody(req): JsonBody<CreateConfigRequest>,
) -> Result<(StatusCode, Json<UserStorageConfigResponse>), ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (bool,): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
{
    if !["github", "gitcode"].contains(&req.provider.as_str()) {
        return Err(error_json(
            locale.0,
            StatusCode::BAD_REQUEST,
            "storage_config.unsupported_provider",
        ));
    }

    check_config_limit(
        state.config.storage_max_user_configs,
        &state.pool,
        user.id,
        locale.0,
    )
    .await?;
    check_name_unique(&state.pool, user.id, &req.name, locale.0).await?;

    verify_repo_access(&req.provider, &req.token, &req.repo, locale.0).await?;

    let encryption_key = state.config.token_encryption_key.as_ref().ok_or_else(|| {
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_config.encryption_key_missing",
        )
    })?;

    let encrypted = encrypt_token_from_config(&req.token, encryption_key).map_err(|e| {
        tracing::warn!("storage config token encryption failed: {e}");
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?;
    let config_json = build_config_json(&req, encrypted);
    let is_default = req.is_default.unwrap_or(false);

    if is_default {
        unset_other_defaults(&state.pool, user.id, locale.0).await?;
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
    .await
    .map_err(|e| {
        tracing::warn!("storage config insert failed: {e}");
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?;

    Ok((StatusCode::CREATED, Json(build_response(&config))))
}

/// GET /api/v1/users/me/storage-configs/{id}
pub async fn get_config<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserStorageConfigResponse>, ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let config = fetch_user_config(&state.pool, id, user.id, locale.0).await?;
    Ok(Json(build_response(&config)))
}

/// PATCH /api/v1/users/me/storage-configs/{id}
pub async fn update_config<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    JsonBody(req): JsonBody<UpdateConfigRequest>,
) -> Result<Json<UserStorageConfigResponse>, ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (bool,): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
{
    let existing = fetch_user_config(&state.pool, id, user.id, locale.0).await?;

    let new_name = req
        .name
        .as_ref()
        .cloned()
        .unwrap_or_else(|| existing.name.clone());
    if new_name != existing.name {
        check_name_unique(&state.pool, user.id, &new_name, locale.0).await?;
    }

    let encrypted_token = if let Some(token) = &req.token {
        let encryption_key = state.config.token_encryption_key.as_ref().ok_or_else(|| {
            error_json(
                locale.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_config.encryption_key_missing",
            )
        })?;
        Some(
            encrypt_token_from_config(token, encryption_key).map_err(|e| {
                tracing::warn!("storage config token encryption failed: {e}");
                error_json(
                    locale.0,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "common.internal_error",
                )
            })?,
        )
    } else {
        None
    };

    let detail = merge_config_detail(existing.config.clone(), &req, encrypted_token);

    let updated = sqlx::query_as::<_, UserStorageConfig>(
        "UPDATE user_storage_configs SET name = $1, config = $2, \
         updated_at = CURRENT_TIMESTAMP \
         WHERE id = $3 AND user_id = $4 \
         RETURNING id, user_id, name, provider, is_default, \
                   config, created_at, updated_at",
    )
    .bind(&new_name)
    .bind(&detail)
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("storage config update failed: {e}");
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?;

    Ok(Json(build_response(&updated)))
}

/// DELETE /api/v1/users/me/storage-configs/{id}
pub async fn delete_config<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (i64,): crate::db::DbRow<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let _existing = fetch_user_config(&state.pool, id, user.id, locale.0).await?;

    let ref_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM images \
         WHERE storage_config_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("storage config reference count failed: {e}");
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?;

    if ref_count > 0 {
        return Err(error_json(
            locale.0,
            StatusCode::CONFLICT,
            "storage_config.in_use",
        ));
    }

    sqlx::query("DELETE FROM user_storage_configs WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("storage config delete failed: {e}");
            error_json(
                locale.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "common.internal_error",
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/users/me/storage-configs/{id}/default
pub async fn set_default<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserStorageConfigResponse>, ApiError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let _existing = fetch_user_config(&state.pool, id, user.id, locale.0).await?;

    unset_other_defaults(&state.pool, user.id, locale.0).await?;

    let config = sqlx::query_as::<_, UserStorageConfig>(
        "UPDATE user_storage_configs SET is_default = true, updated_at = CURRENT_TIMESTAMP \
         WHERE id = $1 AND user_id = $2 \
         RETURNING id, user_id, name, provider, is_default, \
                   config, created_at, updated_at",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("storage config set default failed: {e}");
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?;

    Ok(Json(build_response(&config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_detail(detail: serde_json::Value) -> UserStorageConfig {
        UserStorageConfig {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "git".into(),
            provider: "github".into(),
            is_default: false,
            config: detail,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_build_response_with_detail() {
        let cfg = config_with_detail(serde_json::json!({
            "repo": "owner/repo",
            "branch": "dev",
            "path_prefix": "pic",
            "token_encrypted": "ghp_abcdefgh12345678",
        }));
        let resp = build_response(&cfg);
        assert_eq!(resp.id, cfg.id);
        assert_eq!(resp.name, "git");
        assert_eq!(resp.provider, "github");
        assert_eq!(resp.repo, "owner/repo");
        assert_eq!(resp.branch, "dev");
        assert_eq!(resp.path_prefix.as_deref(), Some("pic"));
        assert_eq!(resp.token_masked, "ghp_****5678");
    }

    #[test]
    fn test_build_response_empty_detail() {
        let cfg = config_with_detail(serde_json::json!({}));
        let resp = build_response(&cfg);
        assert_eq!(resp.repo, "");
        assert_eq!(resp.branch, "main");
        assert!(resp.path_prefix.is_none());
        assert_eq!(resp.token_masked, "****");
    }

    #[test]
    fn test_encrypt_token_from_config() {
        let key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
        let encrypted = encrypt_token_from_config("ghp_token123", key).unwrap();
        assert!(!encrypted.is_empty());
        assert!(encrypt_token_from_config("x", "not-base64!").is_err());
    }

    #[test]
    fn test_build_config_json() {
        let req = CreateConfigRequest {
            name: "git".into(),
            provider: "github".into(),
            token: "t".into(),
            repo: "owner/repo".into(),
            branch: None,
            path_prefix: Some("pic".into()),
            is_default: None,
        };
        let json = build_config_json(&req, "encrypted".into());
        assert_eq!(json["token_encrypted"], "encrypted");
        assert_eq!(json["repo"], "owner/repo");
        assert_eq!(json["branch"], "main");
        assert_eq!(json["path_prefix"], "pic");
    }

    fn update_req(
        repo: Option<&str>,
        branch: Option<&str>,
        path_prefix: Option<&str>,
    ) -> UpdateConfigRequest {
        UpdateConfigRequest {
            name: None,
            token: None,
            repo: repo.map(String::from),
            branch: branch.map(String::from),
            path_prefix: path_prefix.map(String::from),
        }
    }

    #[test]
    fn test_merge_config_detail_partial() {
        let detail = serde_json::json!({"repo": "a/b", "branch": "main"});
        let merged = merge_config_detail(detail, &update_req(Some("c/d"), None, Some("p")), None);
        assert_eq!(merged["repo"], "c/d");
        assert_eq!(merged["branch"], "main");
        assert_eq!(merged["path_prefix"], "p");
        assert!(merged.get("token_encrypted").is_none());
    }

    #[test]
    fn test_merge_config_detail_with_token() {
        let detail = serde_json::json!({"repo": "a/b"});
        let merged = merge_config_detail(
            detail,
            &update_req(None, Some("dev"), None),
            Some("enc".into()),
        );
        assert_eq!(merged["branch"], "dev");
        assert_eq!(merged["repo"], "a/b");
        assert_eq!(merged["token_encrypted"], "enc");
    }

    #[test]
    fn test_api_base_for_provider() {
        assert_eq!(
            api_base_for_provider("github"),
            Some("https://api.github.com")
        );
        assert_eq!(
            api_base_for_provider("gitcode"),
            Some("https://api.gitcode.com/api/v5")
        );
        assert_eq!(api_base_for_provider("other"), None);
    }
}
