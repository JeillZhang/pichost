use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub storage_backend: String,
    pub storage_prefix: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: Uuid,
    pub user_id: Uuid,
    pub public_key: String,
    pub original_name: String,
    pub storage_key: String,
    pub storage_backend: String,
    pub mime_type: String,
    pub file_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub sha256: String,
    pub url: String,
    pub thumbnail_key: Option<String>,
    pub thumbnail_url: Option<String>,
    pub webp_key: Option<String>,
    pub webp_url: Option<String>,
    pub status: ImageStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageStatus {
    Pending,
    Processing,
    Ready,
    Failed,
}

impl std::fmt::Display for ImageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Ready => write!(f, "ready"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadTask {
    pub id: Uuid,
    pub image_id: Uuid,
    pub task_type: String,
    pub payload: Option<serde_json::Value>,
    pub status: String,
    pub error: Option<String>,
    pub retry_count: i32,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
