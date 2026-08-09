use std::sync::Arc;
use pichost_core::DbType;
use sqlx::Pool;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing;
use uuid::Uuid;

use crate::app::AppState;
use crate::cache::{self, Cache};
use crate::i18n_ext::{error_json, JsonBody, Locale};
use pichost_core::config::AppConfig;
use pichost_core::i18n::Language;

// ---- Request / Response types ----

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub invite_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccessTokenClaims {
    pub sub: String,
    pub jti: String,
    pub exp: usize,
    pub iat: usize,
    pub is_admin: bool,
    pub typ: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RefreshTokenClaims {
    pub sub: String,
    pub jti: String,
    pub exp: usize,
    pub iat: usize,
    pub is_admin: bool,
    pub typ: String,
    pub access_jti: String,
    pub access_exp: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub is_admin: bool,
    pub storage_quota: Option<i64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserInfo,
}

// ---- Helpers ----

pub(crate) fn generate_tokens(
    user_id: Uuid,
    is_admin: bool,
    config: &AppConfig,
) -> Result<(String, String, AccessTokenClaims, RefreshTokenClaims), jsonwebtoken::errors::Error> {
    let now = Utc::now().timestamp() as usize;
    let access_exp = now + config.auth.access_token_ttl as usize;
    let refresh_exp = now + config.auth.refresh_token_ttl as usize;

    let access_jti = Uuid::new_v4().to_string();
    let refresh_jti = Uuid::new_v4().to_string();

    let access_claims = AccessTokenClaims {
        sub: user_id.to_string(),
        jti: access_jti.clone(),
        exp: access_exp,
        iat: now,
        is_admin,
        typ: "access".to_string(),
    };

    let refresh_claims = RefreshTokenClaims {
        sub: user_id.to_string(),
        jti: refresh_jti,
        exp: refresh_exp,
        iat: now,
        is_admin,
        typ: "refresh".to_string(),
        access_jti: access_jti.clone(),
        access_exp,
    };

    let key = EncodingKey::from_secret(config.auth.jwt_secret.as_bytes());

    let access_token = encode(&Header::default(), &access_claims, &key)?;
    let refresh_token = encode(&Header::default(), &refresh_claims, &key)?;

    Ok((access_token, refresh_token, access_claims, refresh_claims))
}

async fn check_invite_code<DB: DbType>(
    state: &AppState<DB>,
    invite_code: Option<&str>,
    is_first_user: bool,
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if !is_first_user {
        let code = invite_code
            .ok_or_else(|| error_json(locale, StatusCode::BAD_REQUEST, "invite.required"))?;

        match state.cache.verify_invite_code(code).await.map_err(|e| {
            tracing::warn!("Invite code verification failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })? {
            cache::InviteVerifyResult::Valid => {}
            cache::InviteVerifyResult::Used => {
                return Err(error_json(locale, StatusCode::BAD_REQUEST, "invite.used"));
            }
            cache::InviteVerifyResult::Expired => {
                return Err(error_json(locale, StatusCode::BAD_REQUEST, "invite.expired"));
            }
            cache::InviteVerifyResult::NotFound => {
                return Err(error_json(locale, StatusCode::BAD_REQUEST, "invite.invalid"));
            }
        }
    }
    Ok(())
}

fn hash_password(
    password: &str,
    locale: Language,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::warn!("Password hashing failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })
}

async fn revoke_old_tokens(cache: &Cache, claims: &RefreshTokenClaims, now: usize) {
    let refresh_ttl = claims.exp.saturating_sub(now);
    let _ = cache
        .set_ex(&format!("bl:{}", claims.jti), "revoked", refresh_ttl as u64)
        .await;

    let access_ttl = claims.access_exp.saturating_sub(now);
    if access_ttl > 0 {
        let _ = cache
            .set_ex(
                &format!("bl:{}", claims.access_jti),
                "revoked",
                access_ttl as u64,
            )
            .await;
    }
}

// ---- Handlers ----

async fn count_existing_users<DB: DbType>(
    pool: &Pool<DB>,
    locale: Language,
) -> Result<i64, (StatusCode, Json<serde_json::Value>)>
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
            tracing::warn!("User count query failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })
}

async fn insert_user<DB: DbType>(
    state: &AppState<DB>,
    username: &str,
    email: &Option<String>,
    hash: &str,
    is_admin: bool,
    storage_quota: Option<i64>,
    locale: Language,
) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, is_admin, storage_quota) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(hash)
    .bind(is_admin)
    .bind(storage_quota)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if let Some(code) = db_err.code() {
                if code == "23505" {
                    return error_json(
                        locale,
                        StatusCode::CONFLICT,
                        "auth.username_exists",
                    );
                }
            }
        }
        tracing::warn!("User registration db error: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })
}

pub async fn register<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    JsonBody(payload): JsonBody<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    validate_register_payload(&payload, locale.0)?;
    let user_count = count_existing_users(&state.pool, locale.0).await?;
    let is_first_user = user_count == 0;
    check_invite_code(&state, payload.invite_code.as_deref(), is_first_user, locale.0).await?;
    let (user_id, access_token, refresh_token, storage_quota) =
        create_user_and_tokens(&state, &payload, is_first_user, locale.0).await?;
    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            access_token,
            refresh_token,
            user: UserInfo {
                id: user_id,
                username: payload.username,
                email: payload.email,
                is_admin: is_first_user,
                storage_quota,
            },
        }),
    ))
}

fn validate_register_payload(
    payload: &RegisterRequest,
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if payload.password.len() < 6 {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "validation.password_min_length",
        ));
    }
    Ok(())
}

async fn create_user_and_tokens<DB: DbType>(
    state: &AppState<DB>,
    payload: &RegisterRequest,
    is_first_user: bool,
    locale: Language,
) -> Result<(uuid::Uuid, String, String, Option<i64>), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let hash = hash_password(&payload.password, locale)?;
    let storage_quota = if state.config.upload.storage_quota_default > 0 {
        Some(state.config.upload.storage_quota_default as i64)
    } else {
        None
    };
    let user_id: Uuid = insert_user(
        state,
        &payload.username,
        &payload.email,
        &hash,
        is_first_user,
        storage_quota,
        locale,
    )
    .await?;
    if !is_first_user {
        if let Some(code) = &payload.invite_code {
            let _ = state.cache.consume_invite_code(code, &user_id).await;
        }
    }
    let storage_prefix = format!("users/{}", user_id);
    let _ = sqlx::query("UPDATE users SET storage_prefix = $1 WHERE id = $2")
        .bind(&storage_prefix)
        .bind(user_id)
        .execute(&state.pool)
        .await;
    let (access_token, refresh_token, _ac, _rc) =
        generate_tokens(user_id, is_first_user, &state.config).map_err(|e| {
            tracing::warn!("JWT generation failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })?;
    Ok((user_id, access_token, refresh_token, storage_quota))
}

pub async fn login<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    JsonBody(payload): JsonBody<LoginRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid, String, Option<String>, String, bool, Option<i64>): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    // Query user
    let row = sqlx::query_as::<_, (uuid::Uuid, String, Option<String>, String, bool, Option<i64>)>(
        "SELECT id, username, email, password_hash, is_admin, storage_quota FROM users WHERE username = $1",
    )
    .bind(&payload.username)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Login db query failed: {e}");
        error_json(locale.0, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })?
    .ok_or_else(|| error_json(locale.0, StatusCode::UNAUTHORIZED, "auth.invalid_credentials"))?;

    let (user_id, username, email, password_hash, is_admin, storage_quota) = row;

    // Verify password
    let parsed_hash = PasswordHash::new(&password_hash).map_err(|e| {
        tracing::warn!("Stored password hash parse failed: {e}");
        error_json(locale.0, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })?;

    Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .map_err(|_| error_json(locale.0, StatusCode::UNAUTHORIZED, "auth.invalid_credentials"))?;

    let (access_token, refresh_token, _access_claims, _refresh_claims) =
        generate_tokens(user_id, is_admin, &state.config).map_err(|e| {
            tracing::warn!("JWT generation failed: {e}");
            error_json(locale.0, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
        })?;

    let response = AuthResponse {
        access_token,
        refresh_token,
        user: UserInfo {
            id: user_id,
            username,
            email,
            is_admin,
            storage_quota,
        },
    };

    Ok((StatusCode::OK, Json(response)))
}

async fn lookup_user_for_refresh<DB: DbType>(
    pool: &Pool<DB>,
    sub: &str,
    locale: Language,
) -> Result<(uuid::Uuid, String, Option<String>, bool, Option<i64>), (StatusCode, Json<serde_json::Value>)>

where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let user_id: Uuid = sub
        .parse()
        .map_err(|_| error_json(locale, StatusCode::UNAUTHORIZED, "auth.invalid_subject"))?;
    let row = sqlx::query_as::<_, (String, Option<String>, bool, Option<i64>)>(
        "SELECT username, email, is_admin, storage_quota FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("User lookup failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "auth.internal_error")
    })?
    .ok_or_else(|| error_json(locale, StatusCode::UNAUTHORIZED, "auth.user_not_found"))?;
    Ok((user_id, row.0, row.1, row.2, row.3))
}

pub async fn refresh<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    JsonBody(payload): JsonBody<RefreshRequest>,
) -> Result<(StatusCode, Json<RefreshResponse>), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let config = &state.config;
    let key = DecodingKey::from_secret(config.auth.jwt_secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let token_data = decode::<RefreshTokenClaims>(&payload.refresh_token, &key, &validation)
        .map_err(|_| {
            error_json(locale.0, StatusCode::UNAUTHORIZED, "auth.invalid_refresh_token")
        })?;
    let claims = token_data.claims;
    if claims.typ != "refresh" {
        return Err(error_json(locale.0, StatusCode::UNAUTHORIZED, "auth.invalid_token_type"));
    }
    let bl_refresh_key = format!("bl:{}", claims.jti);
    if state.cache.exists(&bl_refresh_key).await.unwrap_or(true) {
        return Err(error_json(
            locale.0,
            StatusCode::UNAUTHORIZED,
            "auth.refresh_token_revoked",
        ));
    }
    let (user_id, username, email, is_admin, storage_quota) =
        lookup_user_for_refresh(&state.pool, &claims.sub, locale.0).await?;
    let (new_access, new_refresh, _ac, _rc) = generate_tokens(user_id, is_admin, config)
        .map_err(|e| {
            tracing::warn!("Refresh token generation failed: {e}");
            error_json(
                locale.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth.token_generation_failed",
            )
        })?;
    let now = Utc::now().timestamp() as usize;
    revoke_old_tokens(&state.cache, &claims, now).await;
    tracing::info!(user = %user_id, "tokens refreshed (rotation)");
    Ok((StatusCode::OK, Json(RefreshResponse {
        access_token: new_access,
        refresh_token: new_refresh,
        user: UserInfo { id: user_id, username, email, is_admin, storage_quota },
    })))
}

pub async fn logout<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| error_json(locale.0, StatusCode::UNAUTHORIZED, "auth.missing_header"))?;

    let key = DecodingKey::from_secret(state.config.auth.jwt_secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;

    let token_data = decode::<AccessTokenClaims>(token, &key, &validation)
        .map_err(|_| error_json(locale.0, StatusCode::UNAUTHORIZED, "auth.invalid_token"))?;
    let claims = token_data.claims;

    if claims.typ != "access" {
        return Err(error_json(
            locale.0,
            StatusCode::BAD_REQUEST,
            "auth.logout_access_only",
        ));
    }

    let now = Utc::now().timestamp() as usize;
    let ttl = claims.exp.saturating_sub(now);
    if ttl > 0 {
        let bl_key = format!("bl:{}", claims.jti);
        let _ = state.cache.set_ex(&bl_key, "revoked", ttl as u64).await;
    }

    tracing::info!(user = %claims.sub, jti = %claims.jti, "logged out");
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({"message": "logged out successfully"})),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-0123456789abcdef0123456789abcdef";

    fn config_with_secret() -> AppConfig {
        let mut config = AppConfig::default();
        config.auth.jwt_secret = SECRET.into();
        config
    }

    #[test]
    fn test_generate_tokens() {
        let user_id = Uuid::new_v4();
        let (access, refresh, ac, rc) = generate_tokens(user_id, true, &config_with_secret()).unwrap();
        assert_eq!(ac.typ, "access");
        assert_eq!(rc.typ, "refresh");
        assert_eq!(ac.sub, user_id.to_string());
        assert_eq!(rc.sub, user_id.to_string());
        assert_eq!(rc.access_jti, ac.jti);
        assert_eq!(rc.access_exp, ac.exp);
        assert!(ac.is_admin);

        let key = DecodingKey::from_secret(SECRET.as_bytes());
        let decoded: AccessTokenClaims =
            decode(&access, &key, &Validation::new(Algorithm::HS256)).unwrap().claims;
        assert_eq!(decoded.sub, user_id.to_string());
        assert_eq!(decoded.typ, "access");
        let decoded: RefreshTokenClaims =
            decode(&refresh, &key, &Validation::new(Algorithm::HS256)).unwrap().claims;
        assert_eq!(decoded.typ, "refresh");
        assert_eq!(decoded.sub, user_id.to_string());
    }

    #[test]
    fn test_validate_register_payload_short_password() {
        let payload = RegisterRequest {
            username: "u".into(),
            password: "12345".into(),
            email: None,
            invite_code: None,
        };
        let err = validate_register_payload(&payload, Language::En).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_validate_register_payload_ok() {
        let payload = RegisterRequest {
            username: "u".into(),
            password: "123456".into(),
            email: None,
            invite_code: None,
        };
        assert!(validate_register_payload(&payload, Language::En).is_ok());
    }

    #[test]
    fn test_hash_password() {
        let hash = hash_password("password123", Language::En).unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn test_access_claims_serde_roundtrip() {
        let claims = AccessTokenClaims {
            sub: "u1".into(),
            jti: "j1".into(),
            exp: 100,
            iat: 1,
            is_admin: true,
            typ: "access".into(),
        };
        let json = serde_json::to_string(&claims).unwrap();
        let back: AccessTokenClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sub, "u1");
        assert_eq!(back.jti, "j1");
        assert_eq!(back.typ, "access");
        assert!(back.is_admin);
    }

    #[test]
    fn test_refresh_claims_serde_roundtrip() {
        let claims = RefreshTokenClaims {
            sub: "u1".into(),
            jti: "j1".into(),
            exp: 100,
            iat: 1,
            is_admin: false,
            typ: "refresh".into(),
            access_jti: "aj".into(),
            access_exp: 90,
        };
        let json = serde_json::to_string(&claims).unwrap();
        let back: RefreshTokenClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(back.typ, "refresh");
        assert_eq!(back.access_jti, "aj");
        assert_eq!(back.access_exp, 90);
    }
}
