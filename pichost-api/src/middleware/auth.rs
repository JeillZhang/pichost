use std::sync::Arc;

use axum::{
    extract::{Request, State},
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
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {

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
    let token_data = decode::<super::super::routes::auth::AccessTokenClaims>(token, &key, &validation)
        .map_err(|e| {
            tracing::warn!("JWT decode failed: {e}");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "invalid or expired token"})),
            )
        })?;

    let claims = token_data.claims;

    // Check Redis blacklist (fail closed if Redis is down)
    let bl_key = format!("bl:{}", claims.jti);
    let is_revoked = state
        .cache
        .exists(&bl_key)
        .await
        .unwrap_or(true);

    if is_revoked {
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

/// Middleware that rejects non-admin users with 403 Forbidden.
/// MUST be placed after `require_auth` — requires `AuthUser` in extensions.
pub async fn require_admin(
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = req
        .extensions()
        .get::<AuthUser>()
        .ok_or_else(|| {
            tracing::warn!("require_admin called without AuthUser in extensions");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "authentication required"})),
            )
        })?;

    if !auth_user.is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "admin access required"})),
        ));
    }

    Ok(next.run(req).await)
}
