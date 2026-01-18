use crate::config::AppConfig;
use crate::server::{RemoteStats, WifiKeyServer};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Application state shared across commands
pub struct AppState {
    pub server: Arc<Mutex<Option<Arc<WifiKeyServer>>>>,
    pub remote_stats: Arc<RemoteStats>,
    pub config: Arc<Mutex<AppConfig>>,
}
