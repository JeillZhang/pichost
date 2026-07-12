use std::sync::Arc;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing;
use uuid::Uuid;

use crate::app::AppState;
use pichost_core::config::AppConfig;

// ---- Request / Response types ----

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
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
}

#[derive(Debug, Serialize, Clone)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserInfo,
}

// ---- Helpers ----

fn generate_tokens(
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

fn error_response(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({"error": message})))
}

// ---- Handlers ----

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<serde_json::Value>)> {
    if payload.password.len() < 6 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "password must be at least 6 characters",
        ));
    }

    // Hash password
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(|e| {
            tracing::warn!("Password hashing failed: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?
        .to_string();

    // Insert user
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&payload.username)
    .bind(&payload.email)
    .bind(&hash)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if let Some(code) = db_err.code() {
                if code == "23505" {
                    return error_response(
                        StatusCode::CONFLICT,
                        "username or email already exists",
                    );
                }
            }
        }
        tracing::warn!("User registration db error: {e}");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let (access_token, refresh_token, _access_claims, _refresh_claims) =
        generate_tokens(user_id, false, &state.config).map_err(|e| {
            tracing::warn!("JWT generation failed: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?;

    let response = AuthResponse {
        access_token,
        refresh_token,
        user: UserInfo {
            id: user_id,
            username: payload.username,
            email: payload.email,
            is_admin: false,
        },
    };

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<serde_json::Value>)> {
    // Query user
    let row = sqlx::query_as::<_, (Uuid, String, Option<String>, String, bool)>(
        "SELECT id, username, email, password_hash, is_admin FROM users WHERE username = $1",
    )
    .bind(&payload.username)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Login db query failed: {e}");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?
    .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "invalid username or password"))?;

    let (user_id, username, email, password_hash, is_admin) = row;

    // Verify password
    let parsed_hash = PasswordHash::new(&password_hash).map_err(|e| {
        tracing::warn!("Stored password hash parse failed: {e}");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "invalid username or password"))?;

    let (access_token, refresh_token, _access_claims, _refresh_claims) =
        generate_tokens(user_id, is_admin, &state.config).map_err(|e| {
            tracing::warn!("JWT generation failed: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?;

    let response = AuthResponse {
        access_token,
        refresh_token,
        user: UserInfo {
            id: user_id,
            username,
            email,
            is_admin,
        },
    };

    Ok((StatusCode::OK, Json(response)))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RefreshRequest>,
) -> Result<(StatusCode, Json<RefreshResponse>), (StatusCode, Json<serde_json::Value>)> {
    let config = &state.config;
    let key = DecodingKey::from_secret(config.auth.jwt_secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<RefreshTokenClaims>(&payload.refresh_token, &key, &validation)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "invalid or expired refresh token"))?;
    let claims = token_data.claims;

    if claims.typ != "refresh" {
        return Err(error_response(StatusCode::UNAUTHORIZED, "invalid token type"));
    }

    let bl_refresh_key = format!("bl:{}", claims.jti);
    if state.cache.exists(&bl_refresh_key).await.unwrap_or(true) {
        return Err(error_response(StatusCode::UNAUTHORIZED, "refresh token has been revoked"));
    }

    let user_id: Uuid = claims.sub.parse()
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "invalid token subject"))?;

    let row = sqlx::query_as::<_, (String, Option<String>, bool)>(
        "SELECT username, email, is_admin FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Refresh user lookup failed: {e}");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    })?
    .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "user not found"))?;
    let (username, email, is_admin) = row;

    let (new_access, new_refresh, _new_access_claims, _new_refresh_claims) =
        generate_tokens(user_id, is_admin, config)
            .map_err(|e| {
                tracing::warn!("Refresh token generation failed: {e}");
                error_response(StatusCode::INTERNAL_SERVER_ERROR, "token generation failed")
            })?;

    let now = Utc::now().timestamp() as usize;

    let refresh_ttl = claims.exp.saturating_sub(now);
    let _ = state.cache.set_ex(&bl_refresh_key, "revoked", refresh_ttl as u64).await;

    let bl_access_key = format!("bl:{}", claims.access_jti);
    let access_ttl = claims.access_exp.saturating_sub(now);
    if access_ttl > 0 {
        let _ = state.cache.set_ex(&bl_access_key, "revoked", access_ttl as u64).await;
    }

    tracing::info!(user = %user_id, "tokens refreshed (rotation)");

    Ok((
        StatusCode::OK,
        Json(RefreshResponse {
            access_token: new_access,
            refresh_token: new_refresh,
            user: UserInfo { id: user_id, username, email, is_admin },
        }),
    ))
}
