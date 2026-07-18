use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::cache::InviteCodeInfo;
use crate::middleware::auth::AuthUser;
use crate::routes::auth::UserInfo;

// ── Invite Code types ──

#[derive(Debug, Deserialize)]
pub struct CreateInviteBody {
    pub ttl_days: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct CreateInviteResponse {
    pub code: String,
    pub expires_at: i64,
}

// ---- User management types ----

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<UserInfo>,
    pub total: i64,
}

/// GET /api/v1/admin/users — paginated user list (admin only)
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ListUsersResponse>, (StatusCode, Json<serde_json::Value>)> {
    let offset = pagination.offset.unwrap_or(0).max(0);
    let limit = pagination.limit.unwrap_or(50).clamp(1, 200);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin user count query failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            bool,
            String,
            chrono::DateTime<chrono::Utc>,
            Option<i64>,
        ),
    >(
        r#"SELECT id, username, email, is_admin, storage_backend, created_at, storage_quota
           FROM users ORDER BY created_at DESC OFFSET $1 LIMIT $2"#,
    )
    .bind(offset)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin user list query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?;

    let users = rows
        .into_iter()
        .map(
            |(id, username, email, _is_admin, _storage_backend, _created_at, storage_quota)| UserInfo {
                id,
                username,
                email,
                is_admin: _is_admin,
                storage_quota,
            },
        )
        .collect();

    Ok(Json(ListUsersResponse { users, total }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserBody {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
    pub storage_backend: Option<String>,
}

/// PATCH /api/v1/admin/users/{id} — update user fields (admin only)
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<UpdateUserBody>,
) -> Result<Json<UserInfo>, (StatusCode, Json<serde_json::Value>)> {
    // Prevent self-demotion
    if body.is_admin == Some(false) && current_user.id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cannot demote yourself"})),
        ));
    }

    // Fetch existing user
    let existing = sqlx::query_as::<_, (String, Option<String>, bool, String, Option<i64>)>(
        r#"SELECT username, email, is_admin, storage_backend, storage_quota FROM users WHERE id = $1"#,
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin update user query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        )
    })?;

    let (username, email, is_admin, storage_backend, storage_quota) = existing;

    let new_username = body.username.unwrap_or(username);
    let new_email = body.email.or(email);
    let new_is_admin = body.is_admin.unwrap_or(is_admin);
    let new_storage_backend = body.storage_backend.unwrap_or(storage_backend);

    // If password provided, hash it
    if let Some(password) = &body.password {
        if password.len() < 8 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "password must be at least 8 characters"})),
            ));
        }

        use argon2::password_hash::SaltString;
        use argon2::PasswordHasher;
        let salt = SaltString::generate(&mut rand::rngs::OsRng);
        let argon2 = argon2::Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| {
                tracing::warn!("Password hashing failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal error"})),
                )
            })?
            .to_string();

        sqlx::query(
            r#"UPDATE users SET username = $1, email = $2, is_admin = $3,
               storage_backend = $4, password_hash = $5 WHERE id = $6"#,
        )
        .bind(&new_username)
        .bind(&new_email)
        .bind(new_is_admin)
        .bind(&new_storage_backend)
        .bind(&password_hash)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin update user (with pw) failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;
    } else {
        sqlx::query(
            r#"UPDATE users SET username = $1, email = $2, is_admin = $3,
               storage_backend = $4 WHERE id = $5"#,
        )
        .bind(&new_username)
        .bind(&new_email)
        .bind(new_is_admin)
        .bind(&new_storage_backend)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin update user failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;
    }

    tracing::info!(admin_id = %current_user.id, target_user = %user_id, "user updated");

    Ok(Json(UserInfo {
        id: user_id,
        username: new_username,
        email: new_email,
        is_admin: new_is_admin,
        storage_quota,
    }))
}

/// DELETE /api/v1/admin/users/{id} — delete user and all images (admin only)
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    // Prevent self-deletion
    if current_user.id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cannot delete yourself"})),
        ));
    }

    // Verify user exists
    let exists: bool =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(user_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| {
                tracing::warn!("Admin delete user check failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal error"})),
                )
            })?;

    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        ));
    }

    // Collect storage keys for all user's images (to delete physical files)
    let image_keys: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT storage_key, thumbnail_key, webp_key FROM images WHERE user_id = $1"#,
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin delete user image keys query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?;

    // Delete physical files (best-effort — storage errors don't block user deletion)
    let storage = state.router.default_backend();
    for (key, thumb_key, webp_key) in &image_keys {
        let _ = storage.delete(key).await;
        if let Some(tk) = thumb_key {
            let _ = storage.delete(tk).await;
        }
        if let Some(wk) = webp_key {
            let _ = storage.delete(wk).await;
        }
    }

    // Delete from DB (cascade handles images)
    sqlx::query("DELETE FROM images WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin delete user images failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin delete user failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    tracing::info!(admin_id = %current_user.id, target_user = %user_id, images_deleted = image_keys.len(), "user deleted");
    Ok((
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"message": "user deleted"})),
    ))
}

// ---- Admin Stats ----

#[derive(Debug, Serialize)]
pub struct BackendStats {
    pub total_images: i64,
    pub total_size: i64,
}

#[derive(Debug, Serialize)]
pub struct AdminStats {
    pub total_users: i64,
    pub total_images: i64,
    pub total_size: i64,
    pub active_users_24h: i64,
    pub storage_backends: HashMap<String, BackendStats>,
}

/// GET /api/v1/admin/stats — system-wide statistics (admin only, cached 5 min)
pub async fn get_admin_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminStats>, (StatusCode, Json<serde_json::Value>)> {
    // Try cache first using nil UUID as admin stats key
    if let Ok(Some(stats_map)) = state.cache.get_user_stats(&uuid::Uuid::nil()).await {
        if let (Some(total_users), Some(total_images), Some(total_size)) = (
            stats_map.get("total_users").and_then(|v| v.parse().ok()),
            stats_map.get("total_images").and_then(|v| v.parse().ok()),
            stats_map.get("total_size").and_then(|v| v.parse().ok()),
        ) {
            let active_users_24h: i64 = stats_map
                .get("active_users_24h")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let local = BackendStats {
                total_images: stats_map
                    .get("local_images")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(total_images),
                total_size: stats_map
                    .get("local_size")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(total_size),
            };
            let rustfs = BackendStats {
                total_images: stats_map
                    .get("rustfs_images")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                total_size: stats_map
                    .get("rustfs_size")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            };
            let mut backends = HashMap::new();
            backends.insert("local".to_string(), local);
            backends.insert("rustfs".to_string(), rustfs);

            return Ok(Json(AdminStats {
                total_users,
                total_images,
                total_size,
                active_users_24h,
                storage_backends: backends,
            }));
        }
    }

    // Cache miss — query DB
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin stats user count failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    let img_row = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT COUNT(*)::BIGINT as total_images, COALESCE(SUM(file_size), 0)::BIGINT as total_size
           FROM images"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin stats image query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?;
    let (total_images, total_size) = (img_row.0, img_row.1);

    // Active users in last 24h
    let active_users_24h: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT user_id) FROM images
           WHERE created_at > NOW() - INTERVAL '24 hours'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    // Per-backend breakdown
    let local_row = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT COUNT(*)::BIGINT, COALESCE(SUM(file_size), 0)::BIGINT
           FROM images WHERE storage_backend = 'local'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, 0));

    let rustfs_row = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT COUNT(*)::BIGINT, COALESCE(SUM(file_size), 0)::BIGINT
           FROM images WHERE storage_backend = 'rustfs'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, 0));

    let mut backends = HashMap::new();
    backends.insert(
        "local".to_string(),
        BackendStats {
            total_images: local_row.0,
            total_size: local_row.1,
        },
    );
    backends.insert(
        "rustfs".to_string(),
        BackendStats {
            total_images: rustfs_row.0,
            total_size: rustfs_row.1,
        },
    );

    let stats = AdminStats {
        total_users,
        total_images,
        total_size,
        active_users_24h,
        storage_backends: backends,
    };

    // Populate cache (best-effort)
    let nil_uuid = uuid::Uuid::nil();
    let _ = state
        .cache
        .incr_user_stat(&nil_uuid, "total_users", total_users)
        .await;
    let _ = state
        .cache
        .incr_user_stat(&nil_uuid, "total_images", total_images)
        .await;
    let _ = state
        .cache
        .incr_user_stat(&nil_uuid, "total_size", total_size)
        .await;
    let _ = state
        .cache
        .incr_user_stat(&nil_uuid, "active_users_24h", active_users_24h)
        .await;
    let _ = state
        .cache
        .incr_user_stat(&nil_uuid, "local_images", local_row.0)
        .await;
    let _ = state
        .cache
        .incr_user_stat(&nil_uuid, "local_size", local_row.1)
        .await;
    let _ = state
        .cache
        .incr_user_stat(&nil_uuid, "local_size", local_row.1)
        .await;
    let _ = state
        .cache
        .incr_user_stat(&nil_uuid, "rustfs_size", rustfs_row.1)
        .await;

    Ok(Json(stats))
}

// ── Invite Code handlers ──

/// POST /api/v1/admin/invites — create an invite code (admin only)
pub async fn create_invite(
    State(state): State<Arc<AppState>>,
    Extension(admin): Extension<AuthUser>,
    Json(body): Json<CreateInviteBody>,
) -> Result<Json<CreateInviteResponse>, (StatusCode, Json<serde_json::Value>)> {
    let ttl_days = body.ttl_days.unwrap_or(7).clamp(1, 90);
    let ttl_secs = ttl_days * 86400;
    let now = chrono::Utc::now().timestamp();
    let expires_at = now + ttl_secs as i64;

    let code = state
        .cache
        .create_invite_code(&admin.id, ttl_secs)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to create invite code: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    Ok(Json(CreateInviteResponse { code, expires_at }))
}

/// GET /api/v1/admin/invites — list all invite codes (admin only)
pub async fn list_invites(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InviteCodeInfo>>, (StatusCode, Json<serde_json::Value>)> {
    let codes = state.cache.list_invite_codes().await.map_err(|e| {
        tracing::warn!("Failed to list invite codes: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?;
    Ok(Json(codes))
}
