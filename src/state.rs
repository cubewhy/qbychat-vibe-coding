use crate::config::AppConfig;
use crate::gif::GifProvider;
use crate::ws::ServerWsMsg;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceStatus {
    pub status: String, // "online" or "offline"
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub clients: Arc<DashMap<Uuid, mpsc::UnboundedSender<ServerWsMsg>>>,
    pub config: Arc<AppConfig>,
    pub jwt_secret: Arc<String>,
    pub storage_dir: Arc<std::path::PathBuf>,
    pub redis: Option<redis::Client>,
    pub gif_provider: Option<Arc<GifProvider>>,
    pub download_token_ttl: u64,
    pub admin_token: Arc<String>,
    pub typing: Arc<DashMap<(Uuid, Uuid), tokio::time::Instant>>, // (chat_id, user_id) -> typing start time
    pub presence: Arc<DashMap<Uuid, PresenceStatus>>,
    pub sequence_counter: Arc<std::sync::atomic::AtomicU64>,
}
