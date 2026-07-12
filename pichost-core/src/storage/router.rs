use std::collections::HashMap;
use std::sync::Arc;

use super::StorageBackend;

/// Routes storage operations to the appropriate backend based on backend name.
/// Backends are registered at startup and dispatched using the `storage_backend`
/// field stored per-image (and per-user).
pub struct StorageRouter {
    backends: HashMap<String, Arc<dyn StorageBackend>>,
    default: String,
}

impl StorageRouter {
    /// Create a new router with the given backends and default backend name.
    /// If `default` does not match any registered key, the first registered
    /// backend is used as fallback.
    pub fn new(
        backends: HashMap<String, Arc<dyn StorageBackend>>,
        default: String,
    ) -> Self {
        Self { backends, default }
    }

    /// Route to the backend identified by `backend_name`.
    /// Falls back to the default backend if `backend_name` is not registered.
    pub fn for_backend(&self, backend_name: &str) -> &Arc<dyn StorageBackend> {
        self.backends
            .get(backend_name)
            .unwrap_or_else(|| self.default_backend())
    }

    /// Get a backend by exact name. Returns `None` if not found.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn StorageBackend>> {
        self.backends.get(name)
    }

    /// Returns the default backend. Panics if no backends registered.
    pub fn default_backend(&self) -> &Arc<dyn StorageBackend> {
        self.backends
            .get(&self.default)
            .or_else(|| self.backends.values().next())
            .expect("StorageRouter must have at least one backend registered")
    }

    /// Returns the name of the default backend.
    pub fn default_name(&self) -> &str {
        &self.default
    }

    /// Returns the total number of registered backends.
    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::collections::HashMap;
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
        fn backend_name(&self) -> &str { self.0 }
    }

    #[test]
    fn test_router_default_backend() {
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("local".into(), Arc::new(MockBackend("local")));
        backends.insert("rustfs".into(), Arc::new(MockBackend("rustfs")));

        let router = super::StorageRouter::new(backends, "local".into());
        assert_eq!(router.default_backend().backend_name(), "local");
    }

    #[test]
    fn test_router_for_backend() {
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("local".into(), Arc::new(MockBackend("local")));
        backends.insert("rustfs".into(), Arc::new(MockBackend("rustfs")));

        let router = super::StorageRouter::new(backends, "local".into());
        assert_eq!(router.for_backend("rustfs").backend_name(), "rustfs");
        assert_eq!(router.for_backend("nonexistent").backend_name(), "local");
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
