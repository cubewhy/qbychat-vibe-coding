use actix_web::HttpServer;
use anyhow::Context;
use dotenvy::dotenv;
use tracing::info;
use sqlx::PgPool;
use std::sync::Arc;

use qbychat_vibe_coding::run_migrations;
use qbychat_vibe_coding::state::AppState;
use qbychat_vibe_coding::{handlers, ws};
use actix_web::{App, web};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL not set")?;
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "devsecretchangeme".into());

    let pool = PgPool::connect(&database_url).await?;
    run_migrations(&pool).await?;

    let state = AppState { pool, clients: Arc::new(dashmap::DashMap::new()), jwt_secret: Arc::new(jwt_secret) };

    info!("listening on {}", bind_addr);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(handlers::config)
            .service(ws::ws_route)
            .default_service(web::route().to(|| async { actix_web::HttpResponse::NotFound().finish() }))
    })
    .bind(bind_addr)?
    .workers(1)
    .run()
    .await?;

    Ok(())
}
