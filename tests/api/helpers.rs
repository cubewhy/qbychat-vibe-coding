use std::sync::Arc;
use std::net::TcpListener;
use actix_web::{App, web, HttpServer};
use sqlx::postgres::PgPoolOptions;
use qbychat_vibe_coding::{handlers, ws, run_migrations};
use qbychat_vibe_coding::state::AppState;

pub struct TestApp {
    pub address: String,
    pub client: reqwest::Client,
    pub pool: sqlx::PgPool,
    _server: tokio::task::JoinHandle<()>,
}

impl TestApp {
    pub async fn spawn() -> anyhow::Result<Self> {
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://app:password@localhost:5432/app".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect(&db_url)
            .await?;
        run_migrations(&pool).await?;

        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let address = format!("http://127.0.0.1:{}", port);

        std::env::set_var("ADMIN_TOKEN", "test_admin");
        std::fs::create_dir_all("./.test-storage").ok();
        let state = AppState { pool: pool.clone(), clients: Arc::new(dashmap::DashMap::new()), jwt_secret: Arc::new("testsecret".to_string()), storage_dir: Arc::new(std::path::PathBuf::from("./.test-storage")), redis: std::env::var("REDIS_URL").ok().and_then(|u| redis::Client::open(u).ok()) };

        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(state.clone()))
                .configure(handlers::config)
                .service(ws::ws_route)
                .default_service(web::route().to(|| async { actix_web::HttpResponse::NotFound().finish() }))
        })
        .listen(listener)?
        .run();

        let handle = tokio::spawn(async move {
            let _ = server.await;
        });

        let client = reqwest::Client::new();

        Ok(TestApp { address, client, pool, _server: handle })
    }
}
