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
    #[error("payload too large: {0}")]
    PayloadTooLarge(String),
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

    pub fn code(&self) -> &'static str {
        match self {
            Self::Authentication(_) => "auth_failed",
            Self::Authorization(_) => "auth_insufficient_permissions",
            Self::NotFound(_) => "not_found",
            Self::Validation(_) => "validation_error",
            Self::Upload(_) => "upload_failed",
            Self::RateLimited => "rate_limited",
            Self::Storage(e) => match e {
                StorageError::PayloadTooLarge(_) => "storage_payload_too_large",
                _ => "internal_error",
            },
            Self::Internal(_) => "internal_error",
        }
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
            Self::Storage(e) => match e {
                StorageError::PayloadTooLarge(m) => (StatusCode::PAYLOAD_TOO_LARGE, m.clone()),
                _ => {
                    tracing::warn!("{:?}", self);
                    (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
                }
            },
            Self::Internal(_) => {
                tracing::warn!("{:?}", self);
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
        };
        (
            status,
            Json(serde_json::json!({ "error": msg, "code": self.code() })),
        )
            .into_response()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn status(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    #[test]
    fn storage_error_display() {
        assert_eq!(
            StorageError::NotFound("a".into()).to_string(),
            "file not found: a"
        );
        assert_eq!(
            StorageError::WriteFailed("w".into()).to_string(),
            "write failed: w"
        );
        assert_eq!(
            StorageError::ReadFailed("r".into()).to_string(),
            "read failed: r"
        );
        assert_eq!(
            StorageError::ConnectionFailed("c".into()).to_string(),
            "connection failed: c"
        );
        assert_eq!(
            StorageError::Config("cfg".into()).to_string(),
            "config error: cfg"
        );
        assert_eq!(
            StorageError::PayloadTooLarge("big".into()).to_string(),
            "payload too large: big"
        );
    }

    #[test]
    fn app_error_display() {
        assert_eq!(
            AppError::Authentication("x".into()).to_string(),
            "authentication failed: x"
        );
        assert_eq!(
            AppError::Authorization("x".into()).to_string(),
            "not authorized: x"
        );
        assert_eq!(AppError::NotFound("x".into()).to_string(), "not found: x");
        assert_eq!(
            AppError::Validation("x".into()).to_string(),
            "validation failed: x"
        );
        assert_eq!(AppError::Upload("x".into()).to_string(), "upload failed: x");
        assert_eq!(AppError::RateLimited.to_string(), "rate limited");
        assert_eq!(
            AppError::Storage(StorageError::ReadFailed("x".into())).to_string(),
            "storage error: read failed: x"
        );
        assert_eq!(
            AppError::Internal("x".into()).to_string(),
            "internal error: x"
        );
    }

    #[test]
    fn constructors() {
        assert!(matches!(
            AppError::bad_request("m"),
            AppError::Validation(_)
        ));
        assert!(matches!(AppError::not_found("m"), AppError::NotFound(_)));
        assert!(matches!(AppError::internal("m"), AppError::Internal(_)));
    }

    #[test]
    fn into_response_status_map() {
        assert_eq!(
            status(AppError::Authentication("a".into())),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(AppError::Authorization("a".into())),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(AppError::NotFound("a".into())),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(AppError::Validation("a".into())),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status(AppError::Upload("a".into())),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(status(AppError::RateLimited), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            status(AppError::Storage(StorageError::PayloadTooLarge("a".into()))),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            status(AppError::Storage(StorageError::ReadFailed("a".into()))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status(AppError::Storage(StorageError::Config("a".into()))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status(AppError::Internal("a".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn app_error_codes() {
        assert_eq!(AppError::Authentication("a".into()).code(), "auth_failed");
        assert_eq!(AppError::NotFound("a".into()).code(), "not_found");
        assert_eq!(AppError::Validation("a".into()).code(), "validation_error");
        assert_eq!(AppError::RateLimited.code(), "rate_limited");
        assert_eq!(
            AppError::Storage(StorageError::PayloadTooLarge("a".into())).code(),
            "storage_payload_too_large"
        );
        assert_eq!(AppError::Internal("a".into()).code(), "internal_error");
    }

    #[test]
    fn from_sqlx_error() {
        let err: AppError = sqlx::Error::RowNotFound.into();
        assert!(matches!(err, AppError::Internal(msg) if msg == "database error"));
    }

    #[test]
    fn from_crypto_error() {
        let err: AppError = CryptoError::Decrypt.into();
        assert!(matches!(
            err,
            AppError::Internal(msg) if msg == "crypto error: decryption failed: invalid key or corrupted data"
        ));
    }
}
