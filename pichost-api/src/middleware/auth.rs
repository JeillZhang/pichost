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
use crate::routes::auth::AccessTokenClaims;

#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: Uuid,
    pub is_admin: bool,
    pub storage_quota: Option<i64>,
    pub watermark_config: Option<pichost_core::models::WatermarkConfig>,
}

pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let token = extract_bearer_token(&req)?;
    let claims = decode_and_validate_jwt(token, state.config.auth.jwt_secret.as_bytes())?;
    let auth_user = check_blacklist_and_quota(&state, &claims).await?;

    req.extensions_mut().insert(auth_user);
    req.extensions_mut().insert(state);

    Ok(next.run(req).await)
}

fn extract_bearer_token(
    req: &Request,
) -> Result<&str, (StatusCode, Json<serde_json::Value>)> {
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

    auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid authorization format"})),
        )
    })
}

fn decode_and_validate_jwt(
    token: &str,
    secret: &[u8],
) -> Result<AccessTokenClaims, (StatusCode, Json<serde_json::Value>)> {
    let key = DecodingKey::from_secret(secret);
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let token_data =
        decode::<AccessTokenClaims>(token, &key, &validation).map_err(|e| {
            tracing::warn!("JWT decode failed: {e}");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "invalid or expired token"})),
            )
        })?;
    Ok(token_data.claims)
}

async fn check_blacklist_and_quota(
    state: &AppState,
    claims: &AccessTokenClaims,
) -> Result<AuthUser, (StatusCode, Json<serde_json::Value>)> {
    if state.cache.exists(&format!("bl:{}", claims.jti)).await.unwrap_or(true) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "token has been revoked"})),
        ));
    }
    let user_id: Uuid = claims.sub.parse().map_err(|_| {
        (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid token subject"})))
    })?;
    let row = sqlx::query_as::<_, (Option<i64>, Option<serde_json::Value>)>(
        "SELECT storage_quota, watermark_config FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Auth user lookup failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal server error"})))
    })?;

    let (quota, wm_raw) = row.unwrap_or((None, None));
    let watermark_config = wm_raw.and_then(|v| {
        serde_json::from_value::<pichost_core::models::WatermarkConfig>(v).ok()
    });
    Ok(AuthUser { id: user_id, is_admin: claims.is_admin, storage_quota: quota, watermark_config })
}

/// Middleware that rejects non-admin users with 403 Forbidden.
/// MUST be placed after `require_auth` — requires `AuthUser` in extensions.
pub async fn require_admin(
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = req.extensions().get::<AuthUser>().ok_or_else(|| {
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
