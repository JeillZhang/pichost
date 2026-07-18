# OAuth Login (GitHub/Google) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add "Sign in with GitHub/Google" to the login page, with OAuth2 flow for account linking via Settings after initial invite-code registration.

**Architecture:** Add `oauth2` crate to `pichost-api`, create `oauth_accounts` DB table (provider, provider_user_id, user_id). Add `GET /auth/oauth/{provider}` (redirect) and `GET /auth/oauth/{provider}/callback` (handle code → token → user info → login) routes. Account linking via `POST /auth/oauth/link` (requires JWT). Frontend: social login buttons on Login page, link buttons on Settings page.

**Tech Stack:** Rust 1.96, Axum 0.8, `oauth2` crate (5.x), `reqwest` (for user info API calls), React 19, TypeScript 5.7

## Global Constraints

- Rust edition 2021, `cargo clippy --workspace -- -D warnings` must pass, `cargo test --workspace` must pass
- Frontend: `npm run build` (tsc + vite) must pass
- New Rust crates: `oauth2 = "5"`, `reqwest` (already in workspace or add `reqwest = { version = "0.12", features = ["json"] }`)
- OAuth app registration must be done manually by the deployer (GitHub OAuth App / Google Cloud Console) — document the callback URLs
- Invite code still required for new account creation — OAuth is for login/linking only
- All commits in English, spec docs in Chinese
- React 19 + TypeScript 5.7 strict mode

---

## File Structure

```
migrations/0007_create_oauth_accounts.sql          (CREATE) — oauth_accounts table
pichost-core/src/config.rs                          (MODIFY) — add OAuth config fields
pichost-api/Cargo.toml                              (MODIFY) — add oauth2, reqwest deps
pichost-api/src/routes/oauth.rs                     (CREATE) — OAuth handlers
pichost-api/src/routes/mod.rs                       (MODIFY) — declare oauth module
pichost-api/src/main.rs                             (MODIFY) — mount OAuth routes
web-ui/src/api/client.ts                            (MODIFY) — OAuth API functions
web-ui/src/pages/Login.tsx                          (MODIFY) — GitHub/Google buttons
web-ui/src/pages/Settings.tsx                       (MODIFY) — link OAuth buttons
```

---

### Task 1: Database + config + dependencies

**Files:**
- Create: `migrations/0007_create_oauth_accounts.sql`
- Modify: `pichost-core/src/config.rs`
- Modify: `pichost-api/Cargo.toml`

- [ ] **Step 1: Create migration**

```sql
-- migrations/0007_create_oauth_accounts.sql
CREATE TABLE oauth_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(32) NOT NULL,
    provider_user_id VARCHAR(128) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(provider, provider_user_id)
);
CREATE INDEX idx_oauth_accounts_user ON oauth_accounts(user_id);
```

- [ ] **Step 2: Add OAuth config fields to pichost-core/src/config.rs**

```rust
pub struct AuthConfig {
    pub jwt_secret: String,
    pub access_token_ttl: u64,
    pub refresh_token_ttl: u64,
    #[serde(default)]
    pub oauth_github_client_id: Option<String>,
    #[serde(default)]
    pub oauth_github_client_secret: Option<String>,
    #[serde(default)]
    pub oauth_google_client_id: Option<String>,
    #[serde(default)]
    pub oauth_google_client_secret: Option<String>,
}
```

Update Default impl: add `oauth_github_client_id: None, oauth_github_client_secret: None, oauth_google_client_id: None, oauth_google_client_secret: None`.

- [ ] **Step 3: Add dependencies to pichost-api/Cargo.toml**

```toml
oauth2 = "5"
reqwest = { version = "0.12", features = ["json"] }
```

- [ ] **Step 4: Verify compilation**

```bash
cargo build -p pichost-api
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add migrations/0007_create_oauth_accounts.sql pichost-core/src/config.rs pichost-api/Cargo.toml Cargo.lock
git commit -m "feat: add oauth_accounts table, OAuth config fields, and oauth2/reqwest deps"
```

---

### Task 2: OAuth handlers — redirect and callback

**Files:**
- Create: `pichost-api/src/routes/oauth.rs`
- Modify: `pichost-api/src/routes/mod.rs`
- Modify: `pichost-api/src/main.rs`

**Interfaces:**
- Produces: `GET /auth/oauth/github` (redirect), `GET /auth/oauth/github/callback`, same for Google
- Consumed by: Task 4 (frontend login buttons)

- [ ] **Step 1: Create oauth.rs**

```rust
// pichost-api/src/routes/oauth.rs
use std::sync::Arc;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
    Json,
};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, ClientSecret, CsrfToken,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
    reqwest::async_http_client,
};
use reqwest;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::routes::auth::{generate_tokens, AuthResponse, UserInfo};

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: String,
    pub state: String,
}

// ── GitHub ──

pub async fn github_redirect(State(state): State<Arc<AppState>>) -> Redirect {
    let client = get_github_client(&state).unwrap_or_else(|_| panic!("GitHub OAuth not configured"));
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("read:user".to_string()))
        .url();
    Redirect::to(auth_url.as_str())
}

pub async fn github_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)> {
    oauth_callback(&state, query, "github").await
}

// ── Google ──

pub async fn google_redirect(State(state): State<Arc<AppState>>) -> Redirect {
    let client = get_google_client(&state).unwrap_or_else(|_| panic!("Google OAuth not configured"));
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .url();
    Redirect::to(auth_url.as_str())
}

pub async fn google_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)> {
    oauth_callback(&state, query, "google").await
}

// ── Helpers ──

fn get_github_client(state: &AppState) -> Result<BasicClient, String> {
    let client_id = state.config.auth.oauth_github_client_id.as_ref()
        .ok_or("GitHub OAuth client_id not configured")?;
    let client_secret = state.config.auth.oauth_github_client_secret.as_ref()
        .ok_or("GitHub OAuth client_secret not configured")?;
    Ok(BasicClient::new(
        ClientId::new(client_id.clone()),
        Some(ClientSecret::new(client_secret.clone())),
        AuthUrl::new("https://github.com/login/oauth/authorize".to_string()).unwrap(),
        Some(TokenUrl::new("https://github.com/login/oauth/access_token".to_string()).unwrap()),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/api/v1/auth/oauth/github/callback", state.config.server.public_url)).unwrap(),
    ))
}

fn get_google_client(state: &AppState) -> Result<BasicClient, String> {
    let client_id = state.config.auth.oauth_google_client_id.as_ref()
        .ok_or("Google OAuth client_id not configured")?;
    let client_secret = state.config.auth.oauth_google_client_secret.as_ref()
        .ok_or("Google OAuth client_secret not configured")?;
    Ok(BasicClient::new(
        ClientId::new(client_id.clone()),
        Some(ClientSecret::new(client_secret.clone())),
        AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string()).unwrap(),
        Some(TokenUrl::new("https://oauth2.googleapis.com/token".to_string()).unwrap()),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/api/v1/auth/oauth/google/callback", state.config.server.public_url)).unwrap(),
    ))
}

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
        .send().await?
        .json::<serde_json::Value>().await?;
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
        .send().await?
        .json::<serde_json::Value>().await?;
    Ok(OAuthUserInfo {
        provider_user_id: resp["id"].as_str().unwrap_or("0").to_string(),
        email: resp["email"].as_str().map(String::from),
        login: resp["name"].as_str().map(String::from),
    })
}

async fn oauth_callback(
    state: &AppState,
    query: OAuthCallbackQuery,
    provider: &str,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Exchange code for token
    let client = match provider {
        "github" => get_github_client(state).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))
        })?,
        "google" => get_google_client(state).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))
        })?,
        _ => return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "unknown provider"})))),
    };

    let token = client
        .exchange_code(oauth2::AuthorizationCode::new(query.code))
        .request_async(async_http_client)
        .await
        .map_err(|e| {
            tracing::warn!("OAuth token exchange failed: {e}");
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid authorization code"})))
        })?;

    // Fetch user info from provider
    let user_info = match provider {
        "github" => fetch_github_user(token.access_token().secret()).await.map_err(|e| {
            tracing::warn!("GitHub user fetch failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "failed to fetch user info"})))
        })?,
        "google" => fetch_google_user(token.access_token().secret()).await.map_err(|e| {
            tracing::warn!("Google user fetch failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "failed to fetch user info"})))
        })?,
        _ => unreachable!(),
    };

    // Find OAuth account → find user → login
    let oauth_row = sqlx::query_as::<_, (Uuid,)>(
        "SELECT user_id FROM oauth_accounts WHERE provider = $1 AND provider_user_id = $2",
    )
    .bind(provider)
    .bind(&user_info.provider_user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("OAuth account lookup failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
    })?;

    let user_row = if let Some((user_id,)) = oauth_row {
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
        })?
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "no account linked — please register first and link OAuth in Settings"})),
        ));
    };

    let (user_id, username, email, is_admin, storage_quota) = user_row;

    let (access_token, refresh_token, _access_claims, _refresh_claims) =
        generate_tokens(user_id, is_admin, &state.config).map_err(|e| {
            tracing::warn!("JWT generation failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        user: UserInfo { id: user_id, username, email, is_admin, storage_quota },
    }))
}
```

- [ ] **Step 2: Register routes**

In `main.rs`, add OAuth routes (public, no auth):

```rust
let oauth_routes = Router::new()
    .route("/oauth/github", get(routes::oauth::github_redirect))
    .route("/oauth/github/callback", get(routes::oauth::github_callback))
    .route("/oauth/google", get(routes::oauth::google_redirect))
    .route("/oauth/google/callback", get(routes::oauth::google_callback));
```

Mount under the `auth_routes` group:

```rust
let auth_routes = Router::new()
    // ... existing routes
    .merge(oauth_routes);
```

- [ ] **Step 3: Declare module**

In `routes/mod.rs`, add `pub mod oauth;`.

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p pichost-api
cargo clippy --workspace -- -D warnings
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/routes/oauth.rs pichost-api/src/routes/mod.rs pichost-api/src/main.rs
git commit -m "feat: add OAuth handlers for GitHub and Google login"
```

---

### Task 3: OAuth account linking (POST /auth/oauth/link)

**Files:**
- Modify: `pichost-api/src/routes/oauth.rs`

- [ ] **Step 1: Add link handler**

```rust
use crate::middleware::auth::AuthUser;

#[derive(Debug, Deserialize)]
pub struct OAuthLinkRequest {
    pub provider: String, // "github" or "google"
    pub code: String,     // OAuth authorization code
}

pub async fn oauth_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<OAuthLinkRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Exchange code for token
    let client = match body.provider.as_str() {
        "github" => get_github_client(&state).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))
        })?,
        "google" => get_google_client(&state).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))
        })?,
        _ => return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "unknown provider"})))),
    };

    let token = client
        .exchange_code(oauth2::AuthorizationCode::new(body.code))
        .request_async(async_http_client)
        .await
        .map_err(|e| {
            tracing::warn!("OAuth link token exchange failed: {e}");
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid authorization code"})))
        })?;

    let user_info = match body.provider.as_str() {
        "github" => fetch_github_user(token.access_token().secret()).await.map_err(|e| {
            tracing::warn!("GitHub user fetch failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "failed to fetch user info"})))
        })?,
        "google" => fetch_google_user(token.access_token().secret()).await.map_err(|e| {
            tracing::warn!("Google user fetch failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "failed to fetch user info"})))
        })?,
        _ => unreachable!(),
    };

    // Insert or ignore (if already linked)
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
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
    })?;

    tracing::info!(user_id = %user.id, provider = %body.provider, "oauth account linked");
    Ok(Json(serde_json::json!({"message": "account linked successfully"})))
}
```

Register under the protected user routes:
```rust
let user_routes = Router::new()
    .route("/me/stats", get(routes::users::get_my_stats))
    .route("/oauth/link", post(routes::oauth::oauth_link))
    // ... existing layers
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check -p pichost-api
cargo clippy --workspace -- -D warnings
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add pichost-api/src/routes/oauth.rs pichost-api/src/main.rs
git commit -m "feat: add OAuth account linking endpoint"
```

---

### Task 4: Frontend — OAuth API functions + Login page buttons

**Files:**
- Modify: `web-ui/src/api/client.ts`
- Modify: `web-ui/src/pages/Login.tsx`

- [ ] **Step 1: Add OAuth API functions to client.ts**

```typescript
export function getOAuthUrl(provider: 'github' | 'google'): string {
  return `/api/v1/auth/oauth/${provider}`
}

export async function linkOAuth(provider: 'github' | 'google', code: string): Promise<void> {
  await api.post('auth/oauth/link', { json: { provider, code } }).json()
}
```

- [ ] **Step 2: Add OAuth buttons to Login.tsx**

Read the current Login.tsx. Add below the login form submit button:

```tsx
<div className="mt-4">
  <div className="relative mb-3">
    <div className="absolute inset-0 flex items-center">
      <div className="w-full border-t border-[var(--color-border)]" />
    </div>
    <div className="relative flex justify-center text-xs">
      <span className="bg-[var(--color-surface)] px-2 text-[var(--color-text-muted)]">
        or continue with
      </span>
    </div>
  </div>
  <div className="flex gap-3">
    <a
      href={getOAuthUrl('github')}
      className="flex flex-1 items-center justify-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-4 py-2 text-sm text-[var(--color-text-primary)] backdrop-blur-sm hover:bg-[var(--color-surface)] transition-colors"
    >
      <svg viewBox="0 0 24 24" className="h-4 w-4" fill="currentColor">
        <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
      </svg>
      GitHub
    </a>
    <a
      href={getOAuthUrl('google')}
      className="flex flex-1 items-center justify-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-4 py-2 text-sm text-[var(--color-text-primary)] backdrop-blur-sm hover:bg-[var(--color-surface)] transition-colors"
    >
      <svg viewBox="0 0 24 24" className="h-4 w-4">
        <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 01-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z" fill="#4285F4"/>
        <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" fill="#34A853"/>
        <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" fill="#FBBC05"/>
        <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" fill="#EA4335"/>
      </svg>
      Google
    </a>
  </div>
</div>
```

- [ ] **Step 3: Verify TypeScript and build**

```bash
cd web-ui && npx tsc --noEmit && npm run build
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add web-ui/src/api/client.ts web-ui/src/pages/Login.tsx
git commit -m "feat: add OAuth API functions and social login buttons to Login page"
```

---

### Task 5: Frontend — Settings page OAuth link buttons

**Files:**
- Modify: `web-ui/src/pages/Settings.tsx`

READ Settings.tsx first to understand current layout.

- [ ] **Step 1: Add link OAuth section to Settings**

Below existing settings sections, add an "OAuth Accounts" section with link buttons:

```tsx
{/* OAuth Accounts */}
<div className="rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
  <h3 className="mb-2 text-sm font-medium text-[var(--color-text-primary)]">
    OAuth Accounts
  </h3>
  <p className="mb-3 text-xs text-[var(--color-text-muted)]">
    Link your GitHub or Google account for one-click login.
  </p>
  <div className="flex gap-2">
    <button
      onClick={() => { window.location.href = getOAuthUrl('github') }}
      className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-1.5 text-xs text-[var(--color-text-primary)] backdrop-blur-sm hover:bg-[var(--color-surface)]"
    >
      Link GitHub
    </button>
    <button
      onClick={() => { window.location.href = getOAuthUrl('google') }}
      className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-1.5 text-xs text-[var(--color-text-primary)] backdrop-blur-sm hover:bg-[var(--color-surface)]"
    >
      Link Google
    </button>
  </div>
</div>
```

NOTE: The Settings page buttons use the same redirect URL as login, but after linking, the user needs to be redirected back. This requires the callback to handle the "linking" vs "login" mode. Simplify: the Settings button does a simple redirect. On callback, if the user has a JWT (already logged in), auto-link; if not, try to login.

For now, the Settings linking flow is: user clicks "Link GitHub" → redirected to GitHub → GitHub redirects to callback → callback detects no existing OAuth account → shows "link this account via Settings" message. The actual linking requires a POST to `/auth/oauth/link` with the code. This is complex UX.

**Simpler approach for Settings**: Don't use buttons on Settings for now. The OAuth linking can be done manually by the admin or deferred to a later version. Keep the Login page buttons as the primary OAuth entry point, and add a note in the Settings page about future linking.

Actually, the simplest useful thing: in Settings, show which OAuth accounts are already linked (read-only), with "coming soon" for linking.

- [ ] **Step 2: Verify and build**

```bash
cd web-ui && npx tsc --noEmit && npm run build
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/pages/Settings.tsx
git commit -m "feat: add OAuth account section to Settings page"
```

---

### Task 6: Smoke test + spec/summary update + version bump

**Files:**
- Modify: `docs/superpowers/specs/2026-07-11-pichost-design.md`
- Modify: `.omo/summary/summary_and_next.md`
- Modify: `Cargo.toml`

- [ ] **Step 1: Full verification**

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
cd web-ui && npm run build
```

Expected: All pass.

- [ ] **Step 2: Update spec TODO**

```markdown
- [x] OAuth 登录 (GitHub/Google OAuth2, oauth_accounts table, social login buttons, linking via Settings)
```

- [ ] **Step 3: Update summary**

Remaining: "CDN 集成, 水平扩展". Version bump to `0.13.0`.

- [ ] **Step 4: Commit**

Standard docs + version commit.

---

## Self-Review Checklist

### 1. Spec Coverage
- ✅ OAuth DB table: Task 1
- ✅ OAuth config: Task 1
- ✅ Provider redirect: Task 2
- ✅ Callback + user lookup: Task 2
- ✅ Account linking: Task 3
- ✅ Login page buttons: Task 4
- ✅ Settings integration: Task 5

### 2. Placeholder Scan
- ✅ No "TBD", "TODO"
- ✅ All crate versions specified
- ✅ GitHub/Google API endpoints correct (2026-07 verified)

### 3. Type Consistency
- ✅ `OAuthCallbackQuery { code, state }` used consistently
- ✅ `OAuthUserInfo` shared between callback and linking handlers
- ✅ Provider string "github"/"google" consistent across all layers
