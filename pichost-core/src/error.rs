use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use thiserror::Error;

use crate::crypto::CryptoError;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("write failed: {0}")]
    WriteFailed(String),
    #[error("read failed: {0}")]
    ReadFailed(String),
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("config error: {0}")]
    Config(String),
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("authentication failed: {0}")]
    Authentication(String),
    #[error("not authorized: {0}")]
    Authorization(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("upload failed: {0}")]
    Upload(String),
    #[error("rate limited")]
    RateLimited,
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("internal error: {0}")]
    Internal(String),
}

// ── Convenience constructors ────────────────────────────────────────────

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

// ── IntoResponse ────────────────────────────────────────────────────────

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            Self::Authentication(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            Self::Authorization(m) => (StatusCode::FORBIDDEN, m.clone()),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            Self::Validation(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Self::Upload(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            Self::Storage(_) | Self::Internal(_) => {
                tracing::warn!("{:?}", self);
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

// ── Foreign-error conversions ───────────────────────────────────────────

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        tracing::warn!("database error: {e}");
        Self::Internal("database error".into())
    }
}

impl From<CryptoError> for AppError {
    fn from(e: CryptoError) -> Self {
        Self::Internal(format!("crypto error: {e}"))
    }
}
