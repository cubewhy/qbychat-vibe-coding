use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::mpsc;
use sqlx::types::Uuid;
use sqlx::PgPool;
use crate::ws::ServerWsMsg;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub clients: Arc<DashMap<Uuid, mpsc::UnboundedSender<ServerWsMsg>>>,
    pub jwt_secret: Arc<String>,
    pub storage_dir: Arc<std::path::PathBuf>,
    pub redis: Option<redis::Client>,
}
