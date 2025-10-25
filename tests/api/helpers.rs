use actix_web::{web, App, HttpServer};
use dashmap::DashMap;
use qbychat_vibe_coding::config::AppConfig;
use qbychat_vibe_coding::gif::GifProvider;
use qbychat_vibe_coding::state::AppState;
use qbychat_vibe_coding::{handlers, run_migrations, ws};
use sqlx::postgres::PgPoolOptions;
use std::net::TcpListener;
use std::sync::Arc;

pub struct TestApp {
    pub address: String,
    pub client: reqwest::Client,
    pub pool: sqlx::PgPool,
    _server: tokio::task::JoinHandle<()>,
}

impl TestApp {
    pub async fn spawn() -> anyhow::Result<Self> {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://app:password@localhost:5432/app".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect(&db_url)
            .await?;
        run_migrations(&pool).await?;

        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let address = format!("http://127.0.0.1:{}", port);

        let mut config = AppConfig::load()?;
        config.database.url = db_url.clone();
        config.server.bind_addr = format!("127.0.0.1:{}", port);
        config.auth.jwt_secret = "testsecret".into();
        config.admin.token = "test_admin".into();
        config.download.token_ttl_secs = 60;
        config.redis.url = std::env::var("REDIS_URL").ok();
        config.gif.enabled = Some(false);
        let storage_dir = format!("./.test-storage/{}", port);
        std::fs::create_dir_all(&storage_dir).ok();
        config.storage.dir = storage_dir.clone();

        let shared_config = Arc::new(config);
        let redis = shared_config
            .redis
            .url
            .as_ref()
            .and_then(|u| redis::Client::open(u.clone()).ok());
        let gif_provider = GifProvider::from_config(&shared_config.gif).map(Arc::new);

        let state = AppState {
            pool: pool.clone(),
            clients: Arc::new(DashMap::new()),
            config: shared_config.clone(),
            jwt_secret: Arc::new(shared_config.auth.jwt_secret.clone()),
            storage_dir: Arc::new(std::path::PathBuf::from(&shared_config.storage.dir)),
            redis,
            gif_provider,
            download_token_ttl: shared_config.download.token_ttl_secs,
            admin_token: Arc::new(shared_config.admin.token.clone()),
        };

        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(state.clone()))
                .configure(handlers::config)
                .service(ws::ws_route)
                .default_service(
                    web::route().to(|| async { actix_web::HttpResponse::NotFound().finish() }),
                )
        })
        .listen(listener)?
        .run();

        let handle = tokio::spawn(async move {
            let _ = server.await;
        });

        let client = reqwest::Client::new();

        Ok(TestApp {
            address,
            client,
            pool,
            _server: handle,
        })
    }
}
