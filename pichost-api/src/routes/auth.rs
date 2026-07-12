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
use jsonwebtoken::{encode, EncodingKey, Header};
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
