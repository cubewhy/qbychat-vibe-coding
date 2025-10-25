use actix_web::{web, App, HttpServer};
use dotenvy::dotenv;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;

use qbychat_vibe_coding::config::AppConfig;
use qbychat_vibe_coding::gif::GifProvider;
use qbychat_vibe_coding::run_migrations;
use qbychat_vibe_coding::state::AppState;
use qbychat_vibe_coding::{handlers, ws};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Arc::new(AppConfig::load()?);

    let pool = PgPool::connect(&config.database.url).await?;
    run_migrations(&pool).await?;

    tokio::fs::create_dir_all(&config.storage.dir).await.ok();
    let redis = config
        .redis
        .url
        .as_ref()
        .and_then(|url| redis::Client::open(url.clone()).ok());

    let gif_provider = GifProvider::from_config(&config.gif).map(Arc::new);

    let state = AppState {
        pool,
        clients: Arc::new(dashmap::DashMap::new()),
        config: config.clone(),
        jwt_secret: Arc::new(config.auth.jwt_secret.clone()),
        storage_dir: Arc::new(std::path::PathBuf::from(&config.storage.dir)),
        redis,
        gif_provider,
        download_token_ttl: config.download.token_ttl_secs,
        admin_token: Arc::new(config.admin.token.clone()),
        typing: Arc::new(dashmap::DashMap::new()),
        presence: Arc::new(dashmap::DashMap::new()),
        sequence_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    info!("listening on {}", config.server.bind_addr);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(handlers::config)
            .service(ws::ws_route)
            .default_service(
                web::route().to(|| async { actix_web::HttpResponse::NotFound().finish() }),
            )
    })
    .bind(&config.server.bind_addr)?
    .workers(1)
    .run()
    .await?;

    Ok(())
}
