use crate::gif::GifProvider;
use crate::ws::ServerWsMsg;
use dashmap::DashMap;
use sqlx::types::Uuid;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub clients: Arc<DashMap<Uuid, mpsc::UnboundedSender<ServerWsMsg>>>,
    pub jwt_secret: Arc<String>,
    pub storage_dir: Arc<std::path::PathBuf>,
    pub redis: Option<redis::Client>,
    pub gif_provider: Option<Arc<GifProvider>>,
}
