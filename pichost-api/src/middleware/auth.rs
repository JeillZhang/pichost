use std::sync::Arc;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Serialize;
use uuid::Uuid;

use crate::app::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: Uuid,
    pub is_admin: bool,
}

pub async fn require_auth(
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let state = req
        .extensions()
        .get::<Arc<AppState>>()
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal configuration error"})),
            )
        })?;

    // Extract Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "missing authorization header"})),
            )
        })?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "invalid authorization format"})),
            )
        })?;

    // Decode JWT
    let key = DecodingKey::from_secret(state.config.auth.jwt_secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let token_data = decode::<super::super::routes::auth::TokenClaims>(token, &key, &validation)
        .map_err(|e| {
            tracing::warn!("JWT decode failed: {e}");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "invalid or expired token"})),
            )
        })?;

    let claims = token_data.claims;

    // Check Redis blacklist (fail closed if Redis is down)
    let blacklist_key = format!("bl:{}", claims.sub);
    let is_blacklisted = state
        .cache
        .exists(&blacklist_key)
        .await
        .unwrap_or(true);

    if is_blacklisted {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "token has been revoked"})),
        ));
    }

    // Parse user ID and inject into extensions
    let user_id: Uuid = claims.sub.parse().map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid token subject"})),
        )
    })?;

    let auth_user = AuthUser {
        id: user_id,
        is_admin: claims.is_admin,
    };
    req.extensions_mut().insert(auth_user);
    req.extensions_mut().insert(state);

    Ok(next.run(req).await)
}
