use actix_web::{web, App, HttpServer};
use dashmap::DashMap;
use qbychat_vibe_coding::config::AppConfig;
use qbychat_vibe_coding::gif::GifProvider;
use qbychat_vibe_coding::state::AppState;
use qbychat_vibe_coding::{handlers, run_migrations, ws};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::net::TcpListener;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

pub struct TestApp {
    pub address: String,
    pub client: reqwest::Client,
    pub pool: sqlx::PgPool,
    user_counter: std::sync::atomic::AtomicU32,
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
        // Clean up for tests
        sqlx::query("TRUNCATE users CASCADE").execute(&pool).await.ok();
        sqlx::query("TRUNCATE chats CASCADE").execute(&pool).await.ok();
        sqlx::query("TRUNCATE messages CASCADE").execute(&pool).await.ok();
        sqlx::query("TRUNCATE chat_participants CASCADE").execute(&pool).await.ok();
        sqlx::query("TRUNCATE chat_admin_permissions CASCADE").execute(&pool).await.ok();

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
            typing: Arc::new(DashMap::new()),
            presence: Arc::new(DashMap::new()),
        };

        let (tx, rx) = tokio::sync::oneshot::channel();

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
            tx.send(()).ok();
            let _ = server.await;
        });

        // Wait for server to start
        rx.await.ok();
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        let client = reqwest::Client::builder().pool_max_idle_per_host(0).build().unwrap();

        Ok(TestApp {
            address,
            client,
            pool,
            user_counter: AtomicU32::new(0),
            _server: handle,
        })
    }

    pub async fn register_user(&self, base_username: &str) -> anyhow::Result<String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let username = format!("{}_{}", base_username, timestamp);
        let resp = self
            .client
            .post(format!("{}/api/register", self.address))
            .header("Connection", "close")
            .json(&json!({"username": username, "password": "secretpw"}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Registration failed: status {}, body: {}", status, text);
        }
        let token = resp
            .json::<serde_json::Value>()
            .await?
            .get("token")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        Ok(token)
    }

    pub async fn register_user_with_username(&self, base_username: &str) -> anyhow::Result<(String, String)> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let username = format!("{}_{}", base_username, timestamp);
        let resp = self
            .client
            .post(format!("{}/api/register", self.address))
            .header("Connection", "close")
            .json(&json!({"username": username.clone(), "password": "secretpw"}))
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("Registration failed: status {}, body: {}", status, text);
        }
        let v: serde_json::Value = serde_json::from_str(&text)?;
        let token = v
            .get("token")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        Ok((token, username))
    }
}
