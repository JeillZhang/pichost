use std::path::Path;

use pichost_core::config::AppConfig;

/// Runtime `config.toml` view. Every field is optional so a missing file
/// (or missing key) falls back to `pichost-core` figment defaults.
#[derive(Debug, Clone, Default)]
pub struct SystemConfig {
    pub database_url: Option<String>,
    pub redis_url: Option<String>,
    pub jwt_secret: Option<String>,
    pub token_encryption_key: Option<String>,
    pub public_url: Option<String>,
    pub default_backend: Option<String>,
    pub local_base_path: Option<String>,
    pub i18n_language: Option<String>,
    pub i18n_locales_dir: Option<String>,
}

impl SystemConfig {
    /// Build a config.toml view from the runtime-effective config.
    /// Sensitive fields (`jwt_secret`, `token_encryption_key`) are intentionally omitted.
    pub fn from_effective(cfg: &AppConfig) -> Self {
        Self {
            database_url: Some(cfg.database.url.clone()),
            redis_url: Some(cfg.redis.url.clone()),
            jwt_secret: None,
            token_encryption_key: None,
            public_url: Some(cfg.server.public_url.clone()),
            default_backend: Some(cfg.storage.default_backend.clone()),
            local_base_path: Some(cfg.storage.local_base_path.display().to_string()),
            i18n_language: Some(cfg.i18n.language.clone()),
            i18n_locales_dir: cfg
                .i18n
                .locales_dir
                .as_ref()
                .map(|p| p.display().to_string()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Connection error: {0}")]
    Connection(String),
}

/// Read config.toml matching figment's nested key structure.
/// Keys: database.url, redis.url, server.public_url,
///       storage.default_backend, storage.local_base_path, auth.jwt_secret,
///       i18n.language, i18n.locales_dir.
pub fn read_config_toml(path: &Path) -> Result<SystemConfig, ConfigError> {
    if !path.exists() {
        return Ok(SystemConfig::default());
    }
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io(e.to_string()))?;
    let doc: toml_edit::DocumentMut = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ConfigError::Parse(e.to_string()))?;

    fn get_str(doc: &toml_edit::DocumentMut, section: &str, key: &str) -> Option<String> {
        doc.get(section)
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    Ok(SystemConfig {
        database_url: get_str(&doc, "database", "url"),
        redis_url: get_str(&doc, "redis", "url"),
        jwt_secret: get_str(&doc, "auth", "jwt_secret"),
        token_encryption_key: None,
        public_url: get_str(&doc, "server", "public_url"),
        default_backend: get_str(&doc, "storage", "default_backend"),
        local_base_path: get_str(&doc, "storage", "local_base_path"),
        i18n_language: get_str(&doc, "i18n", "language"),
        i18n_locales_dir: get_str(&doc, "i18n", "locales_dir"),
    })
}

/// Write SystemConfig to config.toml using figment-compatible nested keys.
/// Preserves all existing sections/keys.
pub fn write_config_toml(path: &Path, config: &SystemConfig) -> Result<(), ConfigError> {
    let mut doc: toml_edit::DocumentMut = if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io(e.to_string()))?;
        content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| ConfigError::Parse(e.to_string()))?
    } else {
        toml_edit::DocumentMut::new()
    };

    fn set_nested(
        doc: &mut toml_edit::DocumentMut,
        section: &str,
        key: &str,
        val: &Option<String>,
    ) {
        match val {
            Some(v) => {
                doc[section][key] = toml_edit::value(v.as_str());
            }
            None => {
                if let Some(table) = doc.get_mut(section) {
                    table.as_table_mut().map(|t| t.remove(key));
                }
            }
        }
    }

    set_nested(&mut doc, "database", "url", &config.database_url);
    set_nested(&mut doc, "redis", "url", &config.redis_url);
    set_nested(&mut doc, "server", "public_url", &config.public_url);
    set_nested(
        &mut doc,
        "storage",
        "default_backend",
        &config.default_backend,
    );
    set_nested(
        &mut doc,
        "storage",
        "local_base_path",
        &config.local_base_path,
    );
    set_nested(&mut doc, "i18n", "language", &config.i18n_language);
    set_nested(&mut doc, "i18n", "locales_dir", &config.i18n_locales_dir);

    std::fs::write(path, doc.to_string()).map_err(|e| ConfigError::Io(e.to_string()))
}

/// Backup current config.toml to config.toml.{timestamp}.bak
pub fn backup_config(path: &Path) -> Result<String, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::Io("config.toml not found".into()));
    }
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let backup_name = format!("config.toml.{}.bak", ts);
    let backup_path = path.parent().unwrap_or(Path::new(".")).join(&backup_name);
    std::fs::copy(path, &backup_path).map_err(|e| ConfigError::Io(e.to_string()))?;
    Ok(backup_name)
}

/// List all backup files (matches config.toml.*.bak), newest first.
pub fn list_backups(dir: &Path) -> Result<Vec<String>, ConfigError> {
    let mut backups = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| ConfigError::Io(e.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|e| ConfigError::Io(e.to_string()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("config.toml.") && name.ends_with(".bak") {
            backups.push(name);
        }
    }
    backups.sort_by(|a, b| b.cmp(a));
    Ok(backups)
}

/// Restore config.toml from a backup file.
pub fn restore_config(path: &Path, backup_file: &str) -> Result<(), ConfigError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let backup_path = dir.join(backup_file);
    if !backup_path.exists() {
        return Err(ConfigError::Io(format!(
            "Backup not found: {}",
            backup_file
        )));
    }
    std::fs::copy(&backup_path, path).map_err(|e| ConfigError::Io(e.to_string()))?;
    Ok(())
}

/// Test database connection with 5s timeout (connect + SELECT 1).
pub async fn test_database_connection(url: &str) -> Result<(), ConfigError> {
    use std::time::Duration;
    use tokio::time::timeout;

    let result = timeout(Duration::from_secs(5), sqlx::PgPool::connect(url)).await;
    match result {
        Ok(Ok(pool)) => {
            sqlx::query("SELECT 1")
                .execute(&pool)
                .await
                .map_err(|e| ConfigError::Connection(e.to_string()))?;
            pool.close().await;
            Ok(())
        }
        Ok(Err(e)) => Err(ConfigError::Connection(e.to_string())),
        Err(_) => Err(ConfigError::Connection("timed out (5s)".into())),
    }
}

/// Test Redis connection via PING.
pub fn test_redis_connection(url: &str) -> Result<(), ConfigError> {
    let client = redis::Client::open(url).map_err(|e| ConfigError::Connection(e.to_string()))?;
    let mut conn = client
        .get_connection()
        .map_err(|e| ConfigError::Connection(e.to_string()))?;
    let result: String = redis::cmd("PING")
        .query(&mut conn)
        .map_err(|e| ConfigError::Connection(e.to_string()))?;
    if result == "PONG" {
        Ok(())
    } else {
        Err(ConfigError::Connection(format!(
            "unexpected PING: {}",
            result
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_and_read_config_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = SystemConfig {
            database_url: Some("postgresql://test:test@localhost/test".into()),
            redis_url: Some("redis://localhost:6379".into()),
            jwt_secret: None,
            token_encryption_key: None,
            public_url: Some("https://pichost.example.com".into()),
            default_backend: Some("local".into()),
            local_base_path: Some("./test-storage".into()),
            i18n_language: Some("zh-CN".into()),
            i18n_locales_dir: None,
        };
        write_config_toml(&path, &config).unwrap();
        let read = read_config_toml(&path).unwrap();
        assert_eq!(read.database_url, config.database_url);
        assert_eq!(read.public_url, config.public_url);
        assert_eq!(read.default_backend, config.default_backend);
        assert_eq!(read.i18n_language, config.i18n_language);
        assert_eq!(read.i18n_locales_dir, None);
    }

    #[test]
    fn test_read_defaults_when_no_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let config = read_config_toml(&path).unwrap();
        assert_eq!(config.database_url, None);
        assert_eq!(config.public_url, None);
    }

    #[test]
    fn test_from_effective_omits_secrets_and_roundtrips() {
        let app = pichost_core::config::AppConfig::default();
        let view = SystemConfig::from_effective(&app);
        assert!(view.jwt_secret.is_none());
        assert!(view.token_encryption_key.is_none());
        assert_eq!(
            view.database_url.as_deref(),
            Some("postgres://pichost:pichost@localhost:5432/pichost")
        );
        assert_eq!(view.redis_url.as_deref(), Some("redis://localhost:6379"));

        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config_toml(&path, &view).unwrap();
        let read = read_config_toml(&path).unwrap();
        assert_eq!(read.database_url, view.database_url);
        assert_eq!(read.public_url, view.public_url);
        assert_eq!(read.default_backend, view.default_backend);
    }
}

#[cfg(test)]
mod gaps_tests {
    use super::*;
    use tempfile::tempdir;

    fn db_url() -> String {
        std::env::var("PICHOST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://pichost:pichost@localhost:5432/pichost".into())
    }

    fn redis_url() -> String {
        std::env::var("PICHOST_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into())
    }

    #[test]
    fn test_read_existing_file_with_sections() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[database]\nurl = \"postgres://u:p@h:5432/d\"\n[redis]\nurl = \"redis://r:6379\"\n\
             [auth]\njwt_secret = \"secret\"\n[server]\npublic_url = \"https://x\"\n\
             [storage]\ndefault_backend = \"local\"\nlocal_base_path = \"./st\"\n",
        )
        .unwrap();
        let cfg = read_config_toml(&path).unwrap();
        assert_eq!(cfg.database_url.as_deref(), Some("postgres://u:p@h:5432/d"));
        assert_eq!(cfg.redis_url.as_deref(), Some("redis://r:6379"));
        assert_eq!(cfg.jwt_secret.as_deref(), Some("secret"));
        assert_eq!(cfg.public_url.as_deref(), Some("https://x"));
        assert_eq!(cfg.default_backend.as_deref(), Some("local"));
        assert_eq!(cfg.local_base_path.as_deref(), Some("./st"));
    }

    #[test]
    fn test_read_io_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::create_dir(&path).unwrap();
        assert!(matches!(read_config_toml(&path), Err(ConfigError::Io(_))));
    }

    #[test]
    fn test_read_parse_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[database\nurl = ").unwrap();
        assert!(matches!(
            read_config_toml(&path),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn test_write_removes_key_when_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[database]\nurl = \"postgres://u:p@h/d\"\n[redis]\nurl = \"redis://r\"\n",
        )
        .unwrap();
        let partial = SystemConfig {
            database_url: None,
            redis_url: Some("redis://r2".into()),
            ..Default::default()
        };
        write_config_toml(&path, &partial).unwrap();
        let cfg = read_config_toml(&path).unwrap();
        assert_eq!(cfg.database_url, None);
        assert_eq!(cfg.redis_url.as_deref(), Some("redis://r2"));
    }

    #[test]
    fn test_write_preserves_existing_sections() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let partial = SystemConfig {
            database_url: Some("postgres://u:p@h/d".into()),
            ..Default::default()
        };
        write_config_toml(&path, &partial).unwrap();
        let full = SystemConfig {
            database_url: None,
            redis_url: Some("redis://r".into()),
            public_url: Some("https://x".into()),
            ..Default::default()
        };
        write_config_toml(&path, &full).unwrap();
        let cfg = read_config_toml(&path).unwrap();
        assert_eq!(cfg.database_url.as_deref(), Some("postgres://u:p@h/d"));
        assert_eq!(cfg.redis_url.as_deref(), Some("redis://r"));
        assert_eq!(cfg.public_url.as_deref(), Some("https://x"));
    }

    #[test]
    fn test_write_read_error_existing_dir() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::create_dir(&path).unwrap();
        let cfg = SystemConfig::default();
        assert!(matches!(
            write_config_toml(&path, &cfg),
            Err(ConfigError::Io(_))
        ));
    }

    #[test]
    fn test_write_parse_error_existing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not = [valid").unwrap();
        let cfg = SystemConfig::default();
        assert!(matches!(
            write_config_toml(&path, &cfg),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn test_write_io_error() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("afile.txt");
        std::fs::write(&parent, "x").unwrap();
        let path = parent.join("config.toml");
        let cfg = SystemConfig {
            database_url: Some("postgres://u:p@h/d".into()),
            ..Default::default()
        };
        assert!(matches!(
            write_config_toml(&path, &cfg),
            Err(ConfigError::Io(_))
        ));
    }

    #[test]
    fn test_backup_missing_file_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.toml");
        assert!(matches!(backup_config(&path), Err(ConfigError::Io(_))));
    }

    #[test]
    fn test_backup_copy_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::create_dir(&path).unwrap();
        assert!(matches!(backup_config(&path), Err(ConfigError::Io(_))));
    }

    #[test]
    fn test_backup_and_list_filters_sorts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "x = 1").unwrap();
        let name = backup_config(&path).unwrap();
        assert!(name.starts_with("config.toml.") && name.ends_with(".bak"));
        std::fs::write(dir.path().join("config.toml.2026-01-01T00:00:00Z.bak"), "a").unwrap();
        std::fs::write(dir.path().join("other.txt"), "c").unwrap();
        let backups = list_backups(dir.path()).unwrap();
        assert_eq!(backups.len(), 2);
        assert_eq!(backups[0], name);
    }

    #[test]
    fn test_list_backups_invalid_dir() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("afile.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(matches!(list_backups(&file), Err(ConfigError::Io(_))));
    }

    #[test]
    fn test_restore_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "original").unwrap();
        let name = backup_config(&path).unwrap();
        std::fs::write(&path, "changed").unwrap();
        restore_config(&path, &name).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    }

    #[test]
    fn test_restore_missing_backup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let err = restore_config(&path, "config.toml.9999.bak").unwrap_err();
        assert!(matches!(err, ConfigError::Io(_)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_database_connection_ok() {
        assert!(test_database_connection(&db_url()).await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_database_connection_invalid_url() {
        let err = test_database_connection("not-a-valid-url")
            .await
            .unwrap_err();
        assert!(matches!(err, ConfigError::Connection(_)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_database_connection_timeout() {
        let start = std::time::Instant::now();
        let err = test_database_connection("postgres://pichost:pichost@192.0.2.1:5432/pichost")
            .await
            .unwrap_err();
        assert!(matches!(err, ConfigError::Connection(_)));
        assert!(start.elapsed().as_secs() >= 4);
    }

    #[test]
    #[ignore = "requires running PostgreSQL and Redis"]
    fn test_redis_connection_ok() {
        assert!(test_redis_connection(&redis_url()).is_ok());
    }

    #[test]
    fn test_redis_connection_bad_url() {
        assert!(test_redis_connection("not-a-url").is_err());
    }

    #[test]
    fn test_redis_connection_unreachable() {
        assert!(test_redis_connection("redis://127.0.0.1:6399").is_err());
    }

    #[test]
    fn test_redis_connection_unexpected_reply() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 256];
            loop {
                let n = match sock.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                let setinfos = req.matches("SETINFO").count();
                if req.contains("PING") {
                    for _ in 0..setinfos {
                        let _ = sock.write_all(b"+OK\r\n");
                    }
                    let _ = sock.write_all(b"+FOO\r\n");
                } else {
                    for _ in 0..setinfos {
                        let _ = sock.write_all(b"+OK\r\n");
                    }
                }
            }
        });
        let url = format!("redis://{}:{}", addr.ip(), addr.port());
        assert!(test_redis_connection(&url).is_err());
    }

    #[test]
    fn test_redis_connection_eof() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 256];
            let _ = sock.read(&mut buf);
            let _ = sock.write_all(b"+OK\r\n");
        });
        let url = format!("redis://{}:{}", addr.ip(), addr.port());
        assert!(test_redis_connection(&url).is_err());
    }
}
