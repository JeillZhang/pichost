//! Config unit tests: `DatabaseMode` default + `PICHOST_DATABASE_MODE` env parsing.
//!
//! Mirrors the `EnvGuard` snapshot/restore pattern already used inside
//! `config.rs`'s inline `mod tests` (see `test_load_config_env_override_and_restore`).

use crate::config::{AppConfig, DatabaseMode};
use serial_test::serial;
use std::sync::Mutex;

/// Ambient `PICHOST_*` surface captured and cleared by the first `EnvGuard::set`
/// of a test; later `set()` calls are additive, so multi-guard tests keep all
/// their vars while ambient CI vars (e.g. `PICHOST_STORAGE_RUSTFS_*`) never
/// leak into `load_config()`'s figment env layer. Restored on guard drop.
static PICHOST_AMBIENT: Mutex<Option<Vec<(String, Option<String>)>>> =
    Mutex::new(None);

struct EnvGuard {
    key: &'static str,
    old: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let old = std::env::var(key).ok();
        let mut ambient = PICHOST_AMBIENT.lock().unwrap();
        if ambient.is_none() {
            *ambient = Some(
                std::env::vars()
                    .filter(|(k, _)| k.starts_with("PICHOST_"))
                    .map(|(k, v)| {
                        std::env::remove_var(&k);
                        (k, Some(v))
                    })
                    .collect(),
            );
        }
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
        if let Ok(mut ambient) = PICHOST_AMBIENT.lock() {
            if let Some(saved) = ambient.take() {
                for (k, v) in saved {
                    match v {
                        Some(v) => std::env::set_var(k, v),
                        None => std::env::remove_var(k),
                    }
                }
            }
        }
    }
}

#[test]
fn database_mode_defaults_to_postgres() {
    let cfg = AppConfig::default();
    assert!(matches!(cfg.database.mode, DatabaseMode::Postgres));
}

#[serial]
#[test]
fn database_mode_parses_sqlite_from_env() {
    let _mode_guard = EnvGuard::set("PICHOST_DATABASE_MODE", "sqlite");
    let _url_guard = EnvGuard::set("PICHOST_DATABASE_URL", "sqlite:///tmp/test.db");
    let cfg = crate::config::load_config().unwrap();
    assert!(matches!(cfg.database.mode, DatabaseMode::Sqlite));
    assert_eq!(cfg.database.url, "sqlite:///tmp/test.db");
}
