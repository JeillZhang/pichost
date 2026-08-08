use figment::{Figment, providers::{Env, Format, Serialized, Toml}};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub upload: UploadConfig,
    pub logging: LoggingConfig,
    pub worker: WorkerConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    /// AES-256-GCM 密钥，用于加密用户 Git PAT
    /// 须 32 字节（base64 或 hex 编码），与 JWT secret 独立
    #[serde(default)]
    pub token_encryption_key: Option<String>,
    /// 每用户最多可创建的存储配置数。（None = 默认 5）
    #[serde(default)]
    pub storage_max_user_configs: Option<u32>,
    #[serde(default)]
    pub i18n: I18nConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub public_url: String,
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub default_backend: String,
    pub local_base_path: PathBuf,
    #[serde(default)]
    pub rustfs: Option<RustfsStorageConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RustfsStorageConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    #[serde(default = "default_rustfs_region")]
    pub region: String,
    #[serde(default)]
    pub use_ssl: bool,
    #[serde(default)]
    pub public_endpoint: Option<String>,
}

fn default_rustfs_region() -> String {
    "us-east-1".to_string()
}

fn default_storage_quota() -> u64 {
    1_073_741_824 // 1 GB
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UploadConfig {
    pub max_file_size_admin: u64,
    pub max_file_size_user: u64,
    pub allowed_mimes: Vec<String>,
    #[serde(default = "default_storage_quota")]
    pub storage_quota_default: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerProcessingConfig {
    pub thumbnail_size: u32,
    pub thumbnail_quality: u8,
    pub webp_quality: f32,
    pub compress_threshold_kb: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerConfig {
    pub concurrency: usize,
    pub queue_poll_timeout: u64,
    pub task_timeout: u64,
    pub recovery_scan_interval: u64,
    #[serde(default)]
    pub processing: WorkerProcessingConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct I18nConfig {
    #[serde(default = "default_i18n_language")]
    pub language: String,
    #[serde(default)]
    pub locales_dir: Option<PathBuf>,
}

fn default_i18n_language() -> String {
    "en".into()
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            language: default_i18n_language(),
            locales_dir: None,
        }
    }
}

/// Per-policy rate limits (requests per 60s window).
/// Configurable so deployments can tune limits; E2E tests raise them.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit_auth")]
    pub auth_max: u32,
    #[serde(default = "default_rate_limit_upload")]
    pub upload_max: u32,
    #[serde(default = "default_rate_limit_general")]
    pub general_max: u32,
    #[serde(default = "default_rate_limit_public")]
    pub public_max: u32,
}

fn default_rate_limit_auth() -> u32 {
    5
}
fn default_rate_limit_upload() -> u32 {
    30
}
fn default_rate_limit_general() -> u32 {
    60
}
fn default_rate_limit_public() -> u32 {
    200
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            auth_max: default_rate_limit_auth(),
            upload_max: default_rate_limit_upload(),
            general_max: default_rate_limit_general(),
            public_max: default_rate_limit_public(),
        }
    }
}

impl Default for WorkerProcessingConfig {
    fn default() -> Self {
        Self {
            thumbnail_size: 300,
            thumbnail_quality: 85,
            webp_quality: 82.0,
            compress_threshold_kb: 500,
        }
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            queue_poll_timeout: 5,
            task_timeout: 300,
            recovery_scan_interval: 60,
            processing: WorkerProcessingConfig::default(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 3000,
                public_url: "http://localhost:3000".into(),
                cors_origins: vec!["http://localhost:5173".into()],
            },
            auth: AuthConfig {
                jwt_secret: String::new(),
                access_token_ttl: 900,
                refresh_token_ttl: 2_592_000,
                oauth_github_client_id: None,
                oauth_github_client_secret: None,
                oauth_google_client_id: None,
                oauth_google_client_secret: None,
            },
            storage: StorageConfig {
                default_backend: "local".into(),
                local_base_path: PathBuf::from("./storage-local"),
                rustfs: None,
            },
            database: DatabaseConfig {
                url: "postgres://pichost:pichost@localhost:5432/pichost".into(),
                max_connections: 10,
            },
            redis: RedisConfig { url: "redis://localhost:6379".into(), pool_size: 20 },
            upload: UploadConfig {
                max_file_size_admin: 52_428_800,
                max_file_size_user: 10_485_760,
                allowed_mimes: vec![
                    "image/png".into(),
                    "image/jpeg".into(),
                    "image/gif".into(),
                    "image/webp".into(),
                    "image/svg+xml".into(),
                    "image/avif".into(),
                    "image/bmp".into(),
                ],
                storage_quota_default: 1_073_741_824,
            },
            logging: LoggingConfig { level: "info".into(), format: "json".into() },
            worker: WorkerConfig::default(),
            rate_limit: RateLimitConfig::default(),
            token_encryption_key: None,
            storage_max_user_configs: None,
            i18n: I18nConfig::default(),
        }
    }
}

#[allow(clippy::result_large_err)]
pub fn load_config() -> Result<AppConfig, figment::Error> {
    let figment = Figment::new()
        .merge(Serialized::defaults(AppConfig::default()))
        .merge(Toml::file("config.toml").nested())
        // `__` marks explicit nesting: PICHOST_AUTH__JWT_SECRET → auth.jwt_secret.
        // (split("_") alone would produce auth.jwt.secret — three levels — which
        // can never match a flat field name containing underscores.)
        .merge(Env::prefixed("PICHOST_").split("__"))
        // Legacy single-underscore form still works for 2-segment keys like
        // PICHOST_DATABASE_URL → database.url (and for flat keys).
        .merge(Env::prefixed("PICHOST_").split("_"));

    figment.extract()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        key: &'static str,
        old: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let old = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, old }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.old {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    struct PichostEnvGuard {
        saved: Vec<(String, Option<String>)>,
    }

    impl PichostEnvGuard {
        fn capture() -> Self {
            let saved = std::env::vars()
                .filter(|(k, _)| k.starts_with("PICHOST_"))
                .map(|(k, v)| {
                    std::env::remove_var(&k);
                    (k, Some(v))
                })
                .collect();
            Self { saved }
        }
    }

    impl Drop for PichostEnvGuard {
        fn drop(&mut self) {
            for (k, v) in &self.saved {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
        }
    }

    #[test]
    fn test_defaults_i18n_config() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.i18n.language, "en");
        assert!(cfg.i18n.locales_dir.is_none());
    }

    #[test]
    fn test_defaults_app_config() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.server.host, "0.0.0.0");
        assert_eq!(cfg.server.port, 3000);
        assert_eq!(cfg.server.public_url, "http://localhost:3000");
        assert_eq!(cfg.auth.jwt_secret, "");
        assert_eq!(cfg.auth.access_token_ttl, 900);
        assert_eq!(cfg.auth.refresh_token_ttl, 2_592_000);
        assert_eq!(
            cfg.database.url,
            "postgres://pichost:pichost@localhost:5432/pichost"
        );
        assert_eq!(cfg.database.max_connections, 10);
        assert_eq!(cfg.redis.url, "redis://localhost:6379");
        assert_eq!(cfg.redis.pool_size, 20);
        assert_eq!(cfg.storage.default_backend, "local");
        assert_eq!(cfg.storage.local_base_path, PathBuf::from("./storage-local"));
        assert!(cfg.storage.rustfs.is_none());
        assert_eq!(cfg.upload.max_file_size_admin, 52_428_800);
        assert_eq!(cfg.upload.max_file_size_user, 10_485_760);
        assert_eq!(cfg.upload.storage_quota_default, 1_073_741_824);
        assert!(cfg.upload.allowed_mimes.contains(&"image/png".to_string()));
        assert_eq!(cfg.logging.level, "info");
        assert_eq!(cfg.logging.format, "json");
        assert!(cfg.token_encryption_key.is_none());
        assert!(cfg.storage_max_user_configs.is_none());
    }

    #[test]
    fn test_defaults_rate_limit() {
        let rl = RateLimitConfig::default();
        assert_eq!(rl.auth_max, 5);
        assert_eq!(rl.upload_max, 30);
        assert_eq!(rl.general_max, 60);
        assert_eq!(rl.public_max, 200);
    }

    #[test]
    fn test_defaults_worker() {
        let w = WorkerConfig::default();
        assert_eq!(w.concurrency, 4);
        assert_eq!(w.queue_poll_timeout, 5);
        assert_eq!(w.task_timeout, 300);
        assert_eq!(w.recovery_scan_interval, 60);
        let p = WorkerProcessingConfig::default();
        assert_eq!(p.thumbnail_size, 300);
        assert_eq!(p.thumbnail_quality, 85);
        assert_eq!(p.webp_quality, 82.0);
        assert_eq!(p.compress_threshold_kb, 500);
        assert_eq!(w.processing.thumbnail_size, p.thumbnail_size);
    }

    #[test]
    fn test_load_config_env_override_and_restore() {
        let _guard = PichostEnvGuard::capture();
        let g1 = EnvGuard::set("PICHOST_AUTH__JWT_SECRET", "supersecret1234567890");
        let g2 = EnvGuard::set("PICHOST_DATABASE_URL", "postgres://x:y@db:5432/app");
        let cfg = load_config().unwrap();
        assert_eq!(cfg.auth.jwt_secret, "supersecret1234567890");
        assert_eq!(cfg.database.url, "postgres://x:y@db:5432/app");
        drop(g1);
        drop(g2);
        let cfg = load_config().unwrap();
        assert_eq!(cfg.auth.jwt_secret, "");
        assert_eq!(
            cfg.database.url,
            "postgres://pichost:pichost@localhost:5432/pichost"
        );
    }

    #[test]
    fn test_rustfs_config_region_default() {
        let cfg: RustfsStorageConfig = serde_json::from_str(
            r#"{"endpoint":"http://x","bucket":"b","access_key":"a","secret_key":"s"}"#,
        )
        .unwrap();
        assert_eq!(cfg.region, "us-east-1");
        assert!(!cfg.use_ssl);
        assert!(cfg.public_endpoint.is_none());
    }
}
