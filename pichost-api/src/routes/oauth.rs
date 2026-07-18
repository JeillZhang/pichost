use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
    Extension, Json,
};
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl,
    Scope, TokenResponse, TokenUrl,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::middleware::auth::AuthUser;
use crate::routes::auth::{generate_tokens, AuthResponse, UserInfo};

/// Fully-configured OAuth2 client with auth and token endpoints set.
type ConfiguredOAuthClient = oauth2::Client<
    oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>,
    oauth2::StandardTokenResponse<oauth2::EmptyExtraTokenFields, oauth2::basic::BasicTokenType>,
    oauth2::StandardTokenIntrospectionResponse<
        oauth2::EmptyExtraTokenFields,
        oauth2::basic::BasicTokenType,
    >,
    oauth2::StandardRevocableToken,
    oauth2::StandardErrorResponse<oauth2::RevocationErrorResponseType>,
    oauth2::EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
>;

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct OAuthLinkRequest {
    pub provider: String,
    pub code: String,
}

// ── GitHub redirect ──

pub async fn github_redirect(
    State(state): State<Arc<AppState>>,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    let client = make_github_client(&state).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
    })?;
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("read:user".to_string()))
        .add_scope(Scope::new("user:email".to_string()))
        .url();
    Ok(Redirect::to(auth_url.as_str()))
}

// ── Google redirect ──

pub async fn google_redirect(
    State(state): State<Arc<AppState>>,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    let client = make_google_client(&state).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
    })?;
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .url();
    Ok(Redirect::to(auth_url.as_str()))
}

// ── Callbacks ──

pub async fn github_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)> {
    oauth_callback(&state, query, "github").await
}

pub async fn google_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)> {
    oauth_callback(&state, query, "google").await
}

// ── Client builders (return the fully-configured client inline) ──

macro_rules! oauth_client {
    ($state:expr, $client_id_field:ident, $client_secret_field:ident,
     $auth_url:expr, $token_url:expr, $provider:literal) => {{
        let cid = $state
            .config
            .auth
            .$client_id_field
            .as_ref()
            .ok_or(concat!($provider, " OAuth client_id not configured"))?;
        let csec = $state
            .config
            .auth
            .$client_secret_field
            .as_ref()
            .ok_or(concat!($provider, " OAuth client_secret not configured"))?;
        BasicClient::new(ClientId::new(cid.clone()))
            .set_client_secret(ClientSecret::new(csec.clone()))
            .set_auth_uri(
                AuthUrl::new($auth_url.to_string())
                    .expect(concat!("invalid ", $provider, " auth URL")),
            )
            .set_token_uri(
                TokenUrl::new($token_url.to_string())
                    .expect(concat!("invalid ", $provider, " token URL")),
            )
            .set_redirect_uri(
                RedirectUrl::new(format!(
                    "{}/api/v1/auth/oauth/{}/callback",
                    $state.config.server.public_url, $provider
                ))
                .expect(concat!("invalid ", $provider, " redirect URL")),
            )
    }};
}

fn make_github_client(state: &AppState) -> Result<ConfiguredOAuthClient, String> {
    Ok(oauth_client!(
        state,
        oauth_github_client_id,
        oauth_github_client_secret,
        "https://github.com/login/oauth/authorize",
        "https://github.com/login/oauth/access_token",
        "github"
    ))
}

fn make_google_client(state: &AppState) -> Result<ConfiguredOAuthClient, String> {
    Ok(oauth_client!(
        state,
        oauth_google_client_id,
        oauth_google_client_secret,
        "https://accounts.google.com/o/oauth2/v2/auth",
        "https://oauth2.googleapis.com/token",
        "google"
    ))
}

// ── User info fetching ──

#[allow(dead_code)]
struct OAuthUserInfo {
    provider_user_id: String,
    email: Option<String>,
    login: Option<String>,
}

async fn fetch_github_user(token: &str) -> Result<OAuthUserInfo, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "pichost")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    Ok(OAuthUserInfo {
        provider_user_id: resp["id"].as_u64().unwrap_or(0).to_string(),
        email: resp["email"].as_str().map(String::from),
        login: resp["login"].as_str().map(String::from),
    })
}

async fn fetch_google_user(token: &str) -> Result<OAuthUserInfo, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    Ok(OAuthUserInfo {
        provider_user_id: resp["id"].as_str().unwrap_or("0").to_string(),
        email: resp["email"].as_str().map(String::from),
        login: resp["name"].as_str().map(String::from),
    })
}

// ── Shared exchange-code + fetch-user helper ──

async fn oauth_exchange_and_fetch_user(
    state: &AppState,
    provider: &str,
    code: String,
) -> Result<OAuthUserInfo, (StatusCode, Json<serde_json::Value>)> {
    let oauth_client = match provider {
        "github" => make_github_client(state),
        "google" => make_google_client(state),
        _ => Err("unknown provider".to_string()),
    }
    .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;

    let http_client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| {
            tracing::warn!("Failed to build HTTP client: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;

    let token = oauth_client
        .exchange_code(AuthorizationCode::new(code))
        .request_async(&http_client)
        .await
        .map_err(|e| {
            tracing::warn!("OAuth token exchange failed: {e}");
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid authorization code"})))
        })?;

    let access_token = token.access_token().secret();
    match provider {
        "github" => fetch_github_user(access_token).await.map_err(|e| {
            tracing::warn!("GitHub user fetch failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "failed to fetch user info"})))
        }),
        "google" => fetch_google_user(access_token).await.map_err(|e| {
            tracing::warn!("Google user fetch failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "failed to fetch user info"})))
        }),
        _ => unreachable!(),
    }
}

// ── OAuth account → user lookup ──

async fn lookup_oauth_user(
    state: &AppState,
    provider: &str,
    provider_user_id: &str,
) -> Result<(Uuid, String, Option<String>, bool, Option<i64>), (StatusCode, Json<serde_json::Value>)> {
    let oauth_row = sqlx::query_as::<_, (Uuid,)>(
        "SELECT user_id FROM oauth_accounts WHERE provider = $1 AND provider_user_id = $2",
    )
    .bind(provider)
    .bind(provider_user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("OAuth account lookup failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
    })?;

    let (user_id,) = oauth_row.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "no account linked. Register first, then link OAuth in Settings."
        })))
    })?;

    sqlx::query_as::<_, (Uuid, String, Option<String>, bool, Option<i64>)>(
        "SELECT id, username, email, is_admin, storage_quota FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("User lookup failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
    })?
    .ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "user not found"})))
    })
}

// ── Callback handler ──

async fn oauth_callback(
    state: &AppState,
    query: OAuthCallbackQuery,
    provider: &str,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_info = oauth_exchange_and_fetch_user(state, provider, query.code).await?;
    let (user_id, username, email, is_admin, storage_quota) =
        lookup_oauth_user(state, provider, &user_info.provider_user_id).await?;

    let (access_token_str, refresh_token_str, _ac, _rc) =
        generate_tokens(user_id, is_admin, &state.config).map_err(|e| {
            tracing::warn!("JWT generation failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;

    Ok(Json(AuthResponse {
        access_token: access_token_str,
        refresh_token: refresh_token_str,
        user: UserInfo { id: user_id, username, email, is_admin, storage_quota },
    }))
}

// ── OAuth account linking (authenticated user links a provider) ──

pub async fn oauth_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<OAuthLinkRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user_info =
        oauth_exchange_and_fetch_user(&state, &body.provider, body.code).await?;

    sqlx::query(
        r#"INSERT INTO oauth_accounts (user_id, provider, provider_user_id)
           VALUES ($1, $2, $3) ON CONFLICT (provider, provider_user_id) DO NOTHING"#,
    )
    .bind(user.id)
    .bind(&body.provider)
    .bind(&user_info.provider_user_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("OAuth link insert failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?;

    tracing::info!(user_id = %user.id, provider = %body.provider, "oauth account linked");
    Ok(Json(serde_json::json!({"message": "account linked successfully"})))
}
