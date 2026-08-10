use std::sync::Arc;
use pichost_core::DbType;

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

use crate::app::AppState;
use crate::i18n_ext::{error_json, error_json_args, JsonBody, Locale};
use crate::middleware::auth::AuthUser;
use crate::routes::auth::{generate_tokens, AuthResponse, UserInfo};
use pichost_core::i18n::Language;

enum OAuthClientError {
    MissingClientId(&'static str),
    MissingClientSecret(&'static str),
}

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

pub async fn github_redirect<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    let client = make_github_client(&state).map_err(|e| client_error_response(locale.0, e))?;
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("read:user".to_string()))
        .add_scope(Scope::new("user:email".to_string()))
        .url();
    Ok(Redirect::to(auth_url.as_str()))
}

// ── Google redirect ──

pub async fn google_redirect<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    let client = make_google_client(&state).map_err(|e| client_error_response(locale.0, e))?;
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .url();
    Ok(Redirect::to(auth_url.as_str()))
}

// ── Callbacks ──

pub async fn github_callback<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    (uuid::Uuid, String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    oauth_callback(&state, locale.0, query, "github").await
}

pub async fn google_callback<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid, String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    oauth_callback(&state, locale.0, query, "google").await
}

fn client_error_response(
    locale: Language,
    e: OAuthClientError,
) -> (StatusCode, Json<serde_json::Value>) {
    let (key, provider) = match e {
        OAuthClientError::MissingClientId(p) => ("auth.oauth_not_configured", p),
        OAuthClientError::MissingClientSecret(p) => ("auth.oauth_secret_not_configured", p),
    };
    error_json_args(locale, StatusCode::BAD_REQUEST, key, &[provider.to_string()])
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
            .ok_or(OAuthClientError::MissingClientId($provider))?;
        let csec = $state
            .config
            .auth
            .$client_secret_field
            .as_ref()
            .ok_or(OAuthClientError::MissingClientSecret($provider))?;
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

fn make_github_client<DB: DbType>(state: &AppState<DB>) -> Result<ConfiguredOAuthClient, OAuthClientError> {
    Ok(oauth_client!(
        state,
        oauth_github_client_id,
        oauth_github_client_secret,
        "https://github.com/login/oauth/authorize",
        "https://github.com/login/oauth/access_token",
        "github"
    ))
}

fn make_google_client<DB: DbType>(state: &AppState<DB>) -> Result<ConfiguredOAuthClient, OAuthClientError> {
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

async fn oauth_exchange_and_fetch_user<DB: DbType>(
    state: &AppState<DB>,
    locale: Language,
    provider: &str,
    code: String,
) -> Result<OAuthUserInfo, (StatusCode, Json<serde_json::Value>)> {
    let oauth_client = match provider {
        "github" => make_github_client(state).map_err(|e| client_error_response(locale, e)),
        "google" => make_google_client(state).map_err(|e| client_error_response(locale, e)),
        _ => Err(error_json(locale, StatusCode::BAD_REQUEST, "auth.unknown_provider")),
    }?;

    let http_client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| {
            tracing::warn!("Failed to build HTTP client: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "auth.internal_error")
        })?;

    let token = oauth_client
        .exchange_code(AuthorizationCode::new(code))
        .request_async(&http_client)
        .await
        .map_err(|e| {
            tracing::warn!("OAuth token exchange failed: {e}");
            error_json(locale, StatusCode::BAD_REQUEST, "auth.invalid_oauth_code")
        })?;

    let access_token = token.access_token().secret();
    match provider {
        "github" => fetch_github_user(access_token).await.map_err(|e| {
            tracing::warn!("GitHub user fetch failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "auth.oauth_userinfo_failed")
        }),
        "google" => fetch_google_user(access_token).await.map_err(|e| {
            tracing::warn!("Google user fetch failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "auth.oauth_userinfo_failed")
        }),
        _ => unreachable!(),
    }
}

// ── OAuth account → user lookup ──

async fn lookup_oauth_user<DB: DbType>(
    state: &AppState<DB>,
    locale: Language,
    provider: &str,
    provider_user_id: &str,
) -> Result<(uuid::Uuid, String, Option<String>, bool, Option<i64>), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid, String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let oauth_row = sqlx::query_as::<_, (uuid::Uuid,)>(
        "SELECT user_id FROM oauth_accounts WHERE provider = $1 AND provider_user_id = $2",
    )
    .bind(provider)
    .bind(provider_user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("OAuth account lookup failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "auth.internal_error")
    })?;

    let (user_id,) =
        oauth_row.ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "auth.oauth_no_link"))?;

    sqlx::query_as::<_, (uuid::Uuid, String, Option<String>, bool, Option<i64>)>(
        "SELECT id, username, email, is_admin, storage_quota FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("User lookup failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "auth.internal_error")
    })?
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "auth.user_not_found"))
}

// ── Callback handler ──

async fn oauth_callback<DB: DbType>(
    state: &AppState<DB>,
    locale: Language,
    query: OAuthCallbackQuery,
    provider: &str,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid, String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let user_info = oauth_exchange_and_fetch_user(state, locale, provider, query.code).await?;
    let (user_id, username, email, is_admin, storage_quota) =
        lookup_oauth_user(state, locale, provider, &user_info.provider_user_id).await?;

    let (access_token_str, refresh_token_str, _ac, _rc) =
        generate_tokens(user_id, is_admin, &state.config).map_err(|e| {
            tracing::warn!("JWT generation failed: {e}");
            error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "auth.internal_error")
        })?;

    Ok(Json(AuthResponse {
        access_token: access_token_str,
        refresh_token: refresh_token_str,
        user: UserInfo { id: user_id, username, email, is_admin, storage_quota },
    }))
}

// ── OAuth account linking (authenticated user links a provider) ──

pub async fn oauth_link<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Extension(user): Extension<AuthUser>,
    JsonBody(body): JsonBody<OAuthLinkRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let user_info =
        oauth_exchange_and_fetch_user(&state, locale.0, &body.provider, body.code).await?;

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
        error_json(locale.0, StatusCode::INTERNAL_SERVER_ERROR, "auth.internal_error")
    })?;

    tracing::info!(user_id = %user.id, provider = %body.provider, "oauth account linked");
    Ok(Json(serde_json::json!({"message": "account linked successfully"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pichost_core::config::AppConfig;

    /// AppState for unit tests that never touch the DB — in-memory sqlite
    /// pool, no PostgreSQL or Redis connection required.
    async fn unit_test_state() -> AppState<sqlx::Sqlite> {
        use pichost_core::StorageRouter;
        let pool = crate::db::create_sqlite_pool("sqlite::memory:", 1)
            .await
            .expect("sqlite memory pool should build");
        let cache_pool = crate::cache::create_pool("redis://localhost:6379", 2);
        AppState {
            pool,
            cache: Arc::new(crate::cache::Cache::new(cache_pool.clone())),
            blacklist: Arc::new(crate::middleware::auth::RedisBlacklist::new(
                crate::cache::Cache::new(cache_pool.clone()),
            )),
            rate_limiter: Arc::new(
                crate::middleware::rate_limit::RedisRateLimiter::new(crate::cache::Cache::new(
                    cache_pool,
                )),
            ),
            config: Arc::new(AppConfig::default()),
            router: Arc::new(StorageRouter::new(std::collections::HashMap::new(), "local".into())),
        }
    }

    async fn test_state() -> AppState<sqlx::Postgres> {
        use pichost_core::StorageRouter;
        let pool = crate::db::create_pg_pool("postgres://pichost:pichost@localhost:5432/pichost", 2)
            .await
            .expect("pool should connect");
        crate::db::run_pg_migrations(&pool)
            .await
            .expect("migrations should run");
        let cache_pool = crate::cache::create_pool("redis://localhost:6379", 2);
        AppState {
            pool,
            cache: Arc::new(crate::cache::Cache::new(cache_pool.clone())),
            blacklist: Arc::new(crate::middleware::auth::RedisBlacklist::new(
                crate::cache::Cache::new(cache_pool.clone()),
            )),
            rate_limiter: Arc::new(
                crate::middleware::rate_limit::RedisRateLimiter::new(crate::cache::Cache::new(
                    cache_pool,
                )),
            ),
            config: Arc::new(AppConfig::default()),
            router: Arc::new(StorageRouter::new(std::collections::HashMap::new(), "local".into())),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_make_github_client_without_credentials() {
        let state = unit_test_state().await;
        let err = make_github_client(&state).unwrap_err();
        assert!(matches!(err, OAuthClientError::MissingClientId("github")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_make_google_client_without_credentials() {
        let state = unit_test_state().await;
        let err = make_google_client(&state).unwrap_err();
        assert!(matches!(err, OAuthClientError::MissingClientId("google")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_lookup_oauth_user_not_linked() {
        let state = test_state().await;
        let err = lookup_oauth_user(&state, Language::En, "github", "provider-id-1")
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
        assert!(err.1 .0["error"].as_str().unwrap().contains("no account linked"));
    }
}
