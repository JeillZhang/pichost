use thiserror::Error;

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
    #[error("internal error")]
    Internal,
}
