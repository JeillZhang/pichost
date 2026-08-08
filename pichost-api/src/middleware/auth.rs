use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::header::ACCEPT_LANGUAGE,
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use pichost_core::i18n::{I18n, Language};
use serde::Serialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::i18n_ext::{error_json, locale_from_header};
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
    let locale = locale_from_header(req.headers().get(ACCEPT_LANGUAGE), I18n::global().language());
    let token = extract_bearer_token(&req)?;
    let claims = decode_and_validate_jwt(token, state.config.auth.jwt_secret.as_bytes(), locale)?;
    let auth_user = check_blacklist_and_quota(&state, &claims, locale).await?;

    req.extensions_mut().insert(auth_user);
    req.extensions_mut().insert(state);

    Ok(next.run(req).await)
}

fn extract_bearer_token(
    req: &Request,
) -> Result<&str, (StatusCode, Json<serde_json::Value>)> {
    let locale = locale_from_header(req.headers().get(ACCEPT_LANGUAGE), I18n::global().language());
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error_json(locale, StatusCode::UNAUTHORIZED, "auth.missing_header"))?;

    auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| error_json(locale, StatusCode::UNAUTHORIZED, "auth.invalid_header_format"))
}

fn decode_and_validate_jwt(
    token: &str,
    secret: &[u8],
    locale: Language,
) -> Result<AccessTokenClaims, (StatusCode, Json<serde_json::Value>)> {
    let key = DecodingKey::from_secret(secret);
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let token_data = decode::<AccessTokenClaims>(token, &key, &validation).map_err(|e| {
        tracing::warn!("JWT decode failed: {e}");
        error_json(locale, StatusCode::UNAUTHORIZED, "auth.invalid_or_expired_token")
    })?;
    Ok(token_data.claims)
}

async fn check_blacklist_and_quota(
    state: &AppState,
    claims: &AccessTokenClaims,
    locale: Language,
) -> Result<AuthUser, (StatusCode, Json<serde_json::Value>)> {
    if state.cache.exists(&format!("bl:{}", claims.jti)).await.unwrap_or(true) {
        return Err(error_json(locale, StatusCode::UNAUTHORIZED, "auth.token_revoked"));
    }
    let user_id: Uuid = claims
        .sub
        .parse()
        .map_err(|_| error_json(locale, StatusCode::UNAUTHORIZED, "auth.invalid_subject"))?;
    let row = sqlx::query_as::<_, (Option<i64>, Option<serde_json::Value>)>(
        "SELECT storage_quota, watermark_config FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Auth user lookup failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
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
    let locale = locale_from_header(req.headers().get(ACCEPT_LANGUAGE), I18n::global().language());
    let auth_user = req.extensions().get::<AuthUser>().ok_or_else(|| {
        tracing::warn!("require_admin called without AuthUser in extensions");
        error_json(locale, StatusCode::UNAUTHORIZED, "auth.authentication_required")
    })?;

    if !auth_user.is_admin {
        return Err(error_json(locale, StatusCode::FORBIDDEN, "auth.admin_required"));
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use tower::ServiceExt;

    const SECRET: &str = "test-secret-0123456789abcdef0123456789abcdef";

    fn mint_access(sub: &str, is_admin: bool, exp_offset: i64) -> String {
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = AccessTokenClaims {
            sub: sub.to_string(),
            jti: Uuid::new_v4().to_string(),
            exp: (now as i64 + exp_offset) as usize,
            iat: now,
            is_admin,
            typ: "access".to_string(),
        };
        encode(&Header::default(), &claims, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
    }

    #[test]
    fn test_extract_bearer_missing() {
        let req = Request::builder().body(Body::empty()).unwrap();
        let err = extract_bearer_token(&req).unwrap_err();
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_extract_bearer_wrong_format() {
        let req = Request::builder()
            .header("Authorization", "Basic xyz")
            .body(Body::empty())
            .unwrap();
        let err = extract_bearer_token(&req).unwrap_err();
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_extract_bearer_valid() {
        let req = Request::builder()
            .header("Authorization", "Bearer tok123")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req).unwrap(), "tok123");
    }

    #[test]
    fn test_decode_valid_token() {
        let token = mint_access("user-1", true, 900);
        let claims = decode_and_validate_jwt(&token, SECRET.as_bytes(), Language::En).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert!(claims.is_admin);
        assert_eq!(claims.typ, "access");
    }

    #[test]
    fn test_decode_garbage() {
        let err =
            decode_and_validate_jwt("garbage.token.value", SECRET.as_bytes(), Language::En)
                .unwrap_err();
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_decode_expired() {
        let token = mint_access("user-1", false, -100);
        let err = decode_and_validate_jwt(&token, SECRET.as_bytes(), Language::En).unwrap_err();
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }

    fn auth_user(is_admin: bool) -> AuthUser {
        AuthUser {
            id: Uuid::new_v4(),
            is_admin,
            storage_quota: None,
            watermark_config: None,
        }
    }

    async fn inject_admin(mut req: Request, next: Next) -> Response {
        req.extensions_mut().insert(auth_user(true));
        next.run(req).await
    }

    async fn inject_non_admin(mut req: Request, next: Next) -> Response {
        req.extensions_mut().insert(auth_user(false));
        next.run(req).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_require_admin_missing_auth_user() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(require_admin));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_require_admin_non_admin_forbidden() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(require_admin))
            .layer(middleware::from_fn(inject_non_admin));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_require_admin_admin_ok() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(require_admin))
            .layer(middleware::from_fn(inject_admin));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    fn auth_state() -> Arc<crate::app::AppState> {
        use pichost_core::StorageRouter;
        let mut cfg = pichost_core::config::AppConfig::default();
        cfg.auth.jwt_secret = SECRET.to_string();
        Arc::new(crate::app::AppState {
            pool: sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_lazy("postgres://pichost:pichost@localhost:5432/pichost")
                .unwrap(),
            cache: Arc::new(crate::cache::Cache::new(crate::cache::create_pool(
                "redis://localhost:6379",
                2,
            ))),
            config: Arc::new(cfg),
            router: Arc::new(StorageRouter::new(
                std::collections::HashMap::new(),
                "local".into(),
            )),
        })
    }

    fn auth_app(state: Arc<crate::app::AppState>) -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_auth,
            ))
            .with_state(state)
    }

    async fn hit_with_token(app: Router, token: &str) -> StatusCode {
        app.oneshot(
            Request::builder()
                .uri("/")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_require_auth_revoked_token() {
        let state = auth_state();
        let token = mint_access("user-1", false, 900);
        let claims: AccessTokenClaims = decode(
            &token,
            &DecodingKey::from_secret(SECRET.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .unwrap()
        .claims;
        state
            .cache
            .set_ex(&format!("bl:{}", claims.jti), "revoked", 60)
            .await
            .unwrap();
        let status = hit_with_token(auth_app(state), &token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_require_auth_invalid_subject() {
        let token = mint_access("not-a-uuid", false, 900);
        let status = hit_with_token(auth_app(auth_state()), &token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
