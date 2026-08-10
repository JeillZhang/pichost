use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::crypto::decrypt_token;
use crate::models::{GitConfigDetail, UserStorageConfig};

use super::git::{GitProvider, GitStorage};
use super::StorageBackend;
use crate::error::StorageError;

/// Routes storage operations to the appropriate backend based on backend name.
/// Backends are registered at startup and dispatched using the `storage_backend`
/// field stored per-image (and per-user).
///
/// Uses interior mutability (`RwLock<HashMap>`) to allow dynamic creation and
/// caching of Git backends at runtime without restart.
pub struct StorageRouter {
    backends: RwLock<HashMap<String, Arc<dyn StorageBackend>>>,
    default: String,
}

impl StorageRouter {
    /// Create a new router with the given backends and default backend name.
    pub fn new(backends: HashMap<String, Arc<dyn StorageBackend>>, default: String) -> Self {
        Self {
            backends: RwLock::new(backends),
            default,
        }
    }

    /// Route to the backend identified by `backend_name`.
    /// Falls back to the default backend if `backend_name` is not registered.
    pub fn for_backend(&self, backend_name: &str) -> Arc<dyn StorageBackend> {
        self.backends
            .read()
            .ok()
            .and_then(|b| b.get(backend_name).cloned())
            .unwrap_or_else(|| self.default_backend())
    }

    /// Route to the backend identified by user's storage_backend preference.
    /// Falls back to the default backend if the user's preferred backend is
    /// not registered.
    pub fn for_user(&self, backend: &str) -> Arc<dyn StorageBackend> {
        self.backends
            .read()
            .ok()
            .and_then(|b| b.get(backend).cloned())
            .unwrap_or_else(|| self.default_backend())
    }

    /// Get a backend by exact name. Returns `None` if not found.
    pub fn get(&self, name: &str) -> Option<Arc<dyn StorageBackend>> {
        self.backends.read().ok()?.get(name).cloned()
    }

    /// Returns the default backend. Panics if no backends registered.
    pub fn default_backend(&self) -> Arc<dyn StorageBackend> {
        self.backends
            .read()
            .ok()
            .and_then(|b| b.get(&self.default).or_else(|| b.values().next()).cloned())
            .expect("StorageRouter must have at least one backend registered")
    }

    /// Returns the name of the default backend.
    pub fn default_name(&self) -> &str {
        &self.default
    }

    /// Returns the total number of registered backends.
    pub fn backend_count(&self) -> usize {
        self.backends.read().map(|b| b.len()).unwrap_or(0)
    }

    /// Resolve a backend for the given user storage config.
    /// Returns the local default backend for "local" provider, otherwise
    /// checks the cache and dynamically creates a Git backend if needed.
    pub fn for_config(
        &self,
        config: &UserStorageConfig,
        encryption_key: &[u8; 32],
    ) -> Result<Arc<dyn StorageBackend>, StorageError> {
        if config.provider == "local" {
            return Ok(self.default_backend());
        }

        let cache_key = config.id.to_string();
        {
            let backends = self
                .backends
                .read()
                .map_err(|_| StorageError::Config("Router lock poisoned".into()))?;
            if let Some(backend) = backends.get(&cache_key) {
                return Ok(Arc::clone(backend));
            }
        }

        self.get_or_create_git(config, encryption_key)
    }

    /// Dynamically create a GitStorage backend, cache it, and return an Arc.
    pub fn get_or_create_git(
        &self,
        config: &UserStorageConfig,
        encryption_key: &[u8; 32],
    ) -> Result<Arc<dyn StorageBackend>, StorageError> {
        let detail: GitConfigDetail = serde_json::from_value(config.config.clone())
            .map_err(|e| StorageError::Config(format!("config parse failed: {}", e)))?;

        let token = decrypt_token(&detail.token_encrypted, encryption_key)
            .map_err(|e| StorageError::Config(format!("token decrypt failed: {}", e)))?;

        let provider = match config.provider.as_str() {
            "github" => GitProvider::GitHub,
            "gitcode" => GitProvider::GitCode,
            _ => {
                return Err(StorageError::Config(format!(
                    "unknown provider: {}",
                    config.provider
                )))
            }
        };

        let (owner, repo) = detail
            .repo
            .split_once('/')
            .ok_or_else(|| StorageError::Config("repo format error, expected owner/repo".into()))?;

        let git = Arc::new(GitStorage::new(
            provider,
            owner.to_string(),
            repo.to_string(),
            detail.branch,
            detail.path_prefix,
            token,
        )) as Arc<dyn StorageBackend>;

        let mut backends = self
            .backends
            .write()
            .map_err(|_| StorageError::Config("Router lock poisoned".into()))?;
        backends.insert(config.id.to_string(), Arc::clone(&git));

        Ok(git)
    }

    /// Remove a dynamically-created backend from the cache.
    pub fn evict(&self, config_id: &str) {
        if let Ok(mut backends) = self.backends.write() {
            backends.remove(config_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::error::StorageError;

    use super::super::StorageBackend;

    struct MockBackend(&'static str);

    #[async_trait]
    impl StorageBackend for MockBackend {
        async fn put(&self, _key: &str, _data: &[u8], _ct: &str) -> Result<String, StorageError> {
            Ok(self.0.to_string())
        }
        async fn get(&self, _key: &str) -> Result<Vec<u8>, StorageError> {
            Ok(vec![])
        }
        async fn delete(&self, _key: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn exists(&self, _key: &str) -> Result<bool, StorageError> {
            Ok(true)
        }
        fn public_url(&self, _key: &str) -> String {
            format!("http://{}/file", self.0)
        }
        fn backend_name(&self) -> &str {
            self.0
        }
    }

    fn setup_router() -> super::StorageRouter {
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("local".into(), Arc::new(MockBackend("local")));
        backends.insert("rustfs".into(), Arc::new(MockBackend("rustfs")));
        super::StorageRouter::new(backends, "local".into())
    }

    #[test]
    fn test_router_default_backend() {
        let router = setup_router();
        assert_eq!(router.default_backend().backend_name(), "local");
    }

    #[test]
    fn test_router_for_backend() {
        let router = setup_router();
        assert_eq!(router.for_backend("rustfs").backend_name(), "rustfs");
        assert_eq!(router.for_backend("nonexistent").backend_name(), "local");
    }

    #[test]
    fn test_router_for_user() {
        let router = setup_router();
        assert_eq!(router.for_user("rustfs").backend_name(), "rustfs");
        assert_eq!(router.for_user("nonexistent").backend_name(), "local");
    }

    #[test]
    fn test_router_count() {
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("local".into(), Arc::new(MockBackend("local")));

        let router = super::StorageRouter::new(backends, "local".into());
        assert_eq!(router.backend_count(), 1);
    }

    #[test]
    fn test_router_default_name() {
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("local".into(), Arc::new(MockBackend("local")));

        let router = super::StorageRouter::new(backends, "local".into());
        assert_eq!(router.default_name(), "local");
    }

    fn user_config(provider: &str, config: serde_json::Value) -> crate::models::UserStorageConfig {
        crate::models::UserStorageConfig {
            id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            name: "cfg".into(),
            provider: provider.into(),
            is_default: false,
            config,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn git_config(token_encrypted: &str) -> serde_json::Value {
        serde_json::json!({
            "token_encrypted": token_encrypted,
            "repo": "owner/repo",
            "branch": "main",
            "path_prefix": null,
        })
    }

    #[test]
    fn test_router_get() {
        let router = setup_router();
        assert_eq!(router.get("local").unwrap().backend_name(), "local");
        assert_eq!(router.get("rustfs").unwrap().backend_name(), "rustfs");
        assert!(router.get("nope").is_none());
    }

    #[test]
    fn test_router_for_config_local() {
        let router = setup_router();
        let cfg = user_config("local", serde_json::json!({}));
        let backend = router.for_config(&cfg, &[0u8; 32]).unwrap();
        assert_eq!(backend.backend_name(), "local");
    }

    #[test]
    fn test_router_get_or_create_git_success_and_cache() {
        let router = setup_router();
        let key = [7u8; 32];
        let token = crate::crypto::encrypt_token("ghp_token123", &key).unwrap();
        let cfg = user_config("github", git_config(&token));
        let backend = router.get_or_create_git(&cfg, &key).unwrap();
        assert_eq!(backend.backend_name(), "github");
        let again = router.for_config(&cfg, &key).unwrap();
        assert!(Arc::ptr_eq(&backend, &again));
        assert!(router.get(&cfg.id.to_string()).is_some());
    }

    fn expect_config_err(result: Result<Arc<dyn StorageBackend>, StorageError>) -> StorageError {
        match result {
            Err(e) => e,
            Ok(_) => panic!("expected config error"),
        }
    }

    #[test]
    fn test_router_get_or_create_git_invalid_config_json() {
        let router = setup_router();
        let cfg = user_config("github", serde_json::json!({"foo": 1}));
        let err = expect_config_err(router.get_or_create_git(&cfg, &[0u8; 32]));
        assert!(matches!(err, StorageError::Config(_)));
    }

    #[test]
    fn test_router_get_or_create_git_bad_repo_format() {
        let router = setup_router();
        let key = [7u8; 32];
        let token = crate::crypto::encrypt_token("t", &key).unwrap();
        let cfg = user_config(
            "github",
            serde_json::json!({
                "token_encrypted": token,
                "repo": "norepo",
                "branch": "main",
            }),
        );
        let err = expect_config_err(router.get_or_create_git(&cfg, &key));
        assert!(matches!(err, StorageError::Config(_)));
    }

    #[test]
    fn test_router_get_or_create_git_unknown_provider() {
        let router = setup_router();
        let key = [7u8; 32];
        let token = crate::crypto::encrypt_token("t", &key).unwrap();
        let cfg = user_config("bitbucket", git_config(&token));
        let err = expect_config_err(router.get_or_create_git(&cfg, &key));
        assert!(matches!(err, StorageError::Config(_)));
    }

    #[test]
    fn test_router_get_or_create_git_token_decrypt_fails() {
        let router = setup_router();
        let good = [7u8; 32];
        let wrong = [8u8; 32];
        let token = crate::crypto::encrypt_token("secret", &good).unwrap();
        let cfg = user_config("github", git_config(&token));
        let err = expect_config_err(router.get_or_create_git(&cfg, &wrong));
        assert!(matches!(err, StorageError::Config(_)));
    }

    #[test]
    fn test_router_evict() {
        let router = setup_router();
        let key = [7u8; 32];
        let token = crate::crypto::encrypt_token("t", &key).unwrap();
        let cfg = user_config("gitcode", git_config(&token));
        router.get_or_create_git(&cfg, &key).unwrap();
        assert!(router.get(&cfg.id.to_string()).is_some());
        router.evict(&cfg.id.to_string());
        assert!(router.get(&cfg.id.to_string()).is_none());
    }

    #[test]
    fn test_router_default_backend_falls_back_to_first() {
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("rustfs".into(), Arc::new(MockBackend("rustfs")));
        let router = super::StorageRouter::new(backends, "missing".into());
        assert_eq!(router.default_backend().backend_name(), "rustfs");
    }

    #[test]
    #[should_panic(expected = "must have at least one backend")]
    fn test_router_default_backend_panics_when_empty() {
        let router = super::StorageRouter::new(HashMap::new(), "local".into());
        router.default_backend();
    }
}
