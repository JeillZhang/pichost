use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};

use crate::app::AppState;
use crate::middleware::auth::AuthUser;

const POLICY_AUTH: (&str, u32, u64) = ("auth", 5, 60);
const POLICY_UPLOAD: (&str, u32, u64) = ("upload", 30, 60);
const POLICY_GENERAL: (&str, u32, u64) = ("general", 60, 60);
const POLICY_PUBLIC: (&str, u32, u64) = ("public", 200, 60);

fn too_many_response(retry_after: u64) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(serde_json::json!({
            "error": format!("rate limit exceeded, retry after {}s", retry_after)
        })),
    )
}

fn rl_key(policy: &str, suffix: &str) -> String {
    format!("rl:{policy}:{suffix}")
}

fn extract_client_ip(req: &Request) -> String {
    if let Some(xff) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = xff.to_str() {
            if let Some(ip) = val.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }
    "unknown".to_string()
}

async fn check_rate_limit(
    cache: &crate::cache::Cache,
    policy: &str,
    key: &str,
    max_requests: u32,
    window_secs: u64,
) -> Result<u32, u64> {
    let redis_key = rl_key(policy, key);
    match cache.incr(&redis_key, window_secs).await {
        Ok(count) => {
            if count as u32 > max_requests {
                let mut conn = match cache.get_pool().get().await {
                    Ok(c) => c,
                    Err(_) => return Err(window_secs),
                };
                let ttl: u64 = deadpool_redis::redis::cmd("TTL")
                    .arg(&redis_key)
                    .query_async(&mut *conn)
                    .await
                    .unwrap_or(window_secs);
                Err(ttl)
            } else {
                Ok(max_requests - count as u32)
            }
        }
        Err(e) => {
            tracing::warn!("Rate limit Redis error: {e}");
            Ok(max_requests)
        }
    }
}

pub async fn rate_limit_auth(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let ip = extract_client_ip(&req);
    match check_rate_limit(&state.cache, "auth", &ip, POLICY_AUTH.1, POLICY_AUTH.2).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => {
            tracing::warn!(ip = %ip, "auth rate limited");
            Err(too_many_response(retry_after))
        }
    }
}

pub async fn rate_limit_upload(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let key = req
        .extensions()
        .get::<AuthUser>()
        .map(|u| u.id.to_string())
        .unwrap_or_else(|| extract_client_ip(&req));
    match check_rate_limit(
        &state.cache,
        "upload",
        &key,
        POLICY_UPLOAD.1,
        POLICY_UPLOAD.2,
    )
    .await
    {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => {
            tracing::warn!(key = %key, "upload rate limited");
            Err(too_many_response(retry_after))
        }
    }
}

pub async fn rate_limit_general(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let key = req
        .extensions()
        .get::<AuthUser>()
        .map(|u| u.id.to_string())
        .unwrap_or_else(|| extract_client_ip(&req));
    match check_rate_limit(
        &state.cache,
        "general",
        &key,
        POLICY_GENERAL.1,
        POLICY_GENERAL.2,
    )
    .await
    {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => {
            tracing::warn!(key = %key, "general rate limited");
            Err(too_many_response(retry_after))
        }
    }
}

pub async fn rate_limit_public(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let ip = extract_client_ip(&req);
    match check_rate_limit(
        &state.cache,
        "public",
        &ip,
        POLICY_PUBLIC.1,
        POLICY_PUBLIC.2,
    )
    .await
    {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => {
            tracing::warn!(ip = %ip, "public rate limited");
            Err(too_many_response(retry_after))
        }
    }
}
