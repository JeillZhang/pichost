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
    pub fn new(
        backends: HashMap<String, Arc<dyn StorageBackend>>,
        default: String,
    ) -> Self {
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
            .and_then(|b| {
                b.get(&self.default)
                    .or_else(|| b.values().next())
                    .cloned()
            })
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
            let backends = self.backends.read().map_err(|_| {
                StorageError::Config("Router lock poisoned".into())
            })?;
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
            .ok_or_else(|| {
                StorageError::Config(
                    "repo format error, expected owner/repo".into(),
                )
            })?;

        let git = Arc::new(GitStorage::new(
            provider,
            owner.to_string(),
            repo.to_string(),
            detail.branch,
            detail.path_prefix,
            token,
        )) as Arc<dyn StorageBackend>;

        let mut backends = self.backends.write().map_err(|_| {
            StorageError::Config("Router lock poisoned".into())
        })?;
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
        async fn put(
            &self,
            _key: &str,
            _data: &[u8],
            _ct: &str,
        ) -> Result<String, StorageError> {
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
}
