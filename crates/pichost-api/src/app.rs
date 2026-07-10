use std::sync::Arc;

use pichost_core::config::AppConfig;

use crate::cache::Cache;
use crate::db::DbPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub cache: Arc<Cache>,
    pub config: Arc<AppConfig>,
}
