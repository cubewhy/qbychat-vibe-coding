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
            sequence_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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

    pub async fn login(&self, username: &str, password: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(format!("{}/api/login", self.address))
            .header("Connection", "close")
            .json(&json!({"username": username, "password": password}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Login failed: status {}, body: {}", status, text);
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

    pub async fn login_full(&self, username: &str, password: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .post(format!("{}/api/login", self.address))
            .header("Connection", "close")
            .json(&json!({"username": username, "password": password}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Login failed: status {}, body: {}", status, text);
        }
        let response = resp.json::<serde_json::Value>().await?;
        Ok(response)
    }

    pub async fn create_direct_chat(&self, token: &str, peer_username: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(format!("{}/api/chats/direct", self.address))
            .bearer_auth(token)
            .json(&json!({"peer_username": peer_username}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Create direct chat failed: status {}, body: {}", status, text);
        }
        let chat_id = resp
            .json::<serde_json::Value>()
            .await?
            .get("chat_id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        Ok(chat_id)
    }

    pub async fn create_group_chat(&self, token: &str, name: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(format!("{}/api/chats/group", self.address))
            .bearer_auth(token)
            .json(&json!({"name": name}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Create group chat failed: status {}, body: {}", status, text);
        }
        let chat_id = resp
            .json::<serde_json::Value>()
            .await?
            .get("chat_id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        Ok(chat_id)
    }

    pub async fn add_participants(&self, token: &str, chat_id: &str, usernames: Vec<&str>) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/participants", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"usernames": usernames}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Add participants failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn send_message(&self, token: &str, chat_id: &str, content: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/messages", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"content": content}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Send message failed: status {}, body: {}", status, text);
        }
        let message_id = resp
            .json::<serde_json::Value>()
            .await?
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        Ok(message_id)
    }

    pub async fn send_message_with_attachments(&self, token: &str, chat_id: &str, content: &str, attachment_ids: Vec<&str>, reply_to: Option<&str>) -> anyhow::Result<()> {
        let mut body = json!({"content": content, "attachment_ids": attachment_ids});
        if let Some(reply_to_id) = reply_to {
            body["reply_to_message_id"] = json!(reply_to_id);
        }
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/messages", self.address, chat_id))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Send message with attachments failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn edit_message(&self, token: &str, message_id: &str, content: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/messages/{}/edit", self.address, message_id))
            .bearer_auth(token)
            .json(&json!({"content": content}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Edit message failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn delete_message(&self, token: &str, message_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/messages/{}/delete", self.address, message_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Delete message failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn pin_message(&self, token: &str, chat_id: &str, message_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/pin_message", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"message_id": message_id}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Pin message failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn unpin_message(&self, token: &str, chat_id: &str, message_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/unpin_message", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"message_id": message_id}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Unpin message failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn mark_messages_read(&self, token: &str, chat_id: &str, message_ids: Vec<&str>) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/messages/read_bulk", self.address))
            .bearer_auth(token)
            .json(&json!({"chat_id": chat_id, "message_ids": message_ids}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Mark messages read failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn add_admins(&self, token: &str, chat_id: &str, usernames: Vec<&str>) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/admins", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"usernames": usernames}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Add admins failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn remove_participants(&self, token: &str, chat_id: &str, usernames: Vec<&str>) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/remove", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"usernames": usernames}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Remove participants failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn get_messages(&self, token: &str, chat_id: &str, include_reads: bool) -> anyhow::Result<serde_json::Value> {
        let url = if include_reads {
            format!("{}/api/chats/{}/messages?include_reads=true", self.address, chat_id)
        } else {
            format!("{}/api/chats/{}/messages", self.address, chat_id)
        };
        let resp = self
            .client
            .get(url)
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get messages failed: status {}, body: {}", status, text);
        }
        let messages = resp.json::<serde_json::Value>().await?;
        Ok(messages)
    }

    pub async fn get_message_reads(&self, token: &str, message_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/messages/{}/reads", self.address, message_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get message reads failed: status {}, body: {}", status, text);
        }
        let reads = resp.json::<serde_json::Value>().await?;
        Ok(reads)
    }

    pub async fn get_admins(&self, token: &str, chat_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/chats/{}/admins", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get admins failed: status {}, body: {}", status, text);
        }
        let admins = resp.json::<serde_json::Value>().await?;
        Ok(admins)
    }

    pub async fn get_mentions(&self, token: &str, chat_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/chats/{}/member/mentions", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get mentions failed: status {}, body: {}", status, text);
        }
        let mentions = resp.json::<serde_json::Value>().await?;
        Ok(mentions)
    }

    pub async fn search_gifs(&self, token: &str, query: &str, limit: usize) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/gifs/search?q={}&limit={}", self.address, query, limit))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Search GIFs failed: status {}, body: {}", status, text);
        }
        let results = resp.json::<serde_json::Value>().await?;
        Ok(results)
    }

    pub async fn send_gif(&self, token: &str, chat_id: &str, gif_id: &str, gif_url: &str, gif_preview_url: &str, provider: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/gifs", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"gif_id": gif_id, "gif_url": gif_url, "gif_preview_url": gif_preview_url, "provider": provider}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Send GIF failed: status {}, body: {}", status, text);
        }
        let message = resp.json::<serde_json::Value>().await?;
        Ok(message)
    }

    pub async fn set_chat_visibility(&self, token: &str, chat_id: &str, is_public: bool, public_handle: Option<&str>) -> anyhow::Result<()> {
        let mut body = json!({"is_public": is_public});
        if let Some(handle) = public_handle {
            body["public_handle"] = json!(handle);
        }
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/visibility", self.address, chat_id))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Set chat visibility failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn search_public_chats(&self, token: &str, handle: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/chats/public_search?handle={}", self.address, handle))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Search public chats failed: status {}, body: {}", status, text);
        }
        let results = resp.json::<serde_json::Value>().await?;
        Ok(results)
    }

    pub async fn join_public_chat(&self, token: &str, handle: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .post(format!("{}/api/chats/public_join", self.address))
            .bearer_auth(token)
            .json(&json!({"handle": handle}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Join public chat failed: status {}, body: {}", status, text);
        }
        let result = resp.json::<serde_json::Value>().await?;
        Ok(result)
    }

    pub async fn mute_user(&self, token: &str, chat_id: &str, username: &str, minutes: u32) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/mute", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"username": username, "minutes": minutes}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Mute user failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn unmute_user(&self, token: &str, chat_id: &str, username: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/unmute", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"username": username}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Unmute user failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn remove_admin(&self, token: &str, chat_id: &str, username: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/admins/remove", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"username": username}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Remove admin failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn clear_messages(&self, token: &str, chat_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/clear_messages", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Clear messages failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn get_chat_list(&self, token: &str, include_unread: bool, include_first: bool) -> anyhow::Result<serde_json::Value> {
        let mut query = vec![];
        if include_unread {
            query.push("include_unread=true");
        }
        if include_first {
            query.push("include_first=true");
        }
        let query_str = if query.is_empty() { String::new() } else { format!("?{}", query.join("&")) };
        let resp = self
            .client
            .get(format!("{}/api/chats{}", self.address, query_str))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get chat list failed: status {}, body: {}", status, text);
        }
        let chats = resp.json::<serde_json::Value>().await?;
        Ok(chats)
    }

    pub async fn set_notify_settings(&self, token: &str, chat_id: &str, mute_forever: bool, mute_until: Option<&str>, notify_type: &str) -> anyhow::Result<()> {
        let mut body = json!({"mute_forever": mute_forever, "notify_type": notify_type});
        if let Some(until) = mute_until {
            body["mute_until"] = json!(until);
        }
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/member/notify", self.address, chat_id))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Set notify settings failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn get_notify_settings(&self, token: &str, chat_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/chats/{}/member/notify", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get notify settings failed: status {}, body: {}", status, text);
        }
        let settings = resp.json::<serde_json::Value>().await?;
        Ok(settings)
    }

    pub async fn get_mention_ids(&self, token: &str, chat_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/chats/{}/member/mentions", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get mention IDs failed: status {}, body: {}", status, text);
        }
        let mentions = resp.json::<serde_json::Value>().await?;
        Ok(mentions)
    }

    pub async fn clear_mentions(&self, token: &str, chat_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .delete(format!("{}/api/chats/{}/member/mentions", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Clear mentions failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn upload_file(&self, token: &str, file_data: Vec<u8>, filename: &str, mime_type: &str) -> anyhow::Result<String> {
        let part = reqwest::multipart::Part::bytes(file_data)
            .file_name(filename.to_string())
            .mime_str(mime_type)
            .unwrap();
        let form = reqwest::multipart::Form::new().part("f1", part);
        let resp = self
            .client
            .post(format!("{}/api/files", self.address))
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Upload file failed: status {}, body: {}", status, text);
        }
        let files: Vec<serde_json::Value> = resp.json().await?;
        let file_id = files[0]["id"].as_str().unwrap().to_string();
        Ok(file_id)
    }

    pub async fn get_unread_count(&self, token: &str, chat_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/chats/{}/unread_count", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get unread count failed: status {}, body: {}", status, text);
        }
        let count = resp.json::<serde_json::Value>().await?;
        Ok(count)
    }

    pub async fn set_member_note(&self, token: &str, chat_id: &str, note: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/member/note", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"note": note}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Set member note failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn get_member_note(&self, token: &str, chat_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}/api/chats/{}/member/note", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get member note failed: status {}, body: {}", status, text);
        }
        let note = resp.json::<serde_json::Value>().await?;
        Ok(note)
    }

    pub async fn delete_member_note(&self, token: &str, chat_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .delete(format!("{}/api/chats/{}/member/note", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Delete member note failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn leave_chat(&self, token: &str, chat_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/leave", self.address, chat_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Leave chat failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn forward_messages(&self, token: &str, target_chat_id: &str, from_chat_id: &str, message_ids: Vec<&str>) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/forward_messages", self.address, target_chat_id))
            .bearer_auth(token)
            .json(&json!({"from_chat_id": from_chat_id, "message_ids": message_ids}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Forward messages failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn upload_avatars(&self, token: &str, avatars: Vec<(Vec<u8>, &str)>) -> anyhow::Result<Vec<String>> {
        let mut form = reqwest::multipart::Form::new();
        for (i, (data, filename)) in avatars.iter().enumerate() {
            let part = reqwest::multipart::Part::bytes(data.clone())
                .file_name(filename.to_string())
                .mime_str("image/png")
                .unwrap();
            form = form.part(format!("file{}", i + 1), part);
        }
        let resp = self
            .client
            .post(format!("{}/api/users/me/avatars", self.address))
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Upload avatars failed: status {}, body: {}", status, text);
        }
        let uploaded: Vec<serde_json::Value> = resp.json().await?;
        let ids = uploaded.iter().map(|v| v["id"].as_str().unwrap().to_string()).collect();
        Ok(ids)
    }

    pub async fn set_primary_avatar(&self, token: &str, avatar_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/users/me/avatars/primary", self.address))
            .bearer_auth(token)
            .json(&json!({"avatar_id": avatar_id}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Set primary avatar failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn get_user_avatars(&self, token: &str, user_id: &str) -> anyhow::Result<Vec<serde_json::Value>> {
        let resp = self
            .client
            .get(format!("{}/api/users/{}/avatars", self.address, user_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get user avatars failed: status {}, body: {}", status, text);
        }
        let avatars: Vec<serde_json::Value> = resp.json().await?;
        Ok(avatars)
    }

    pub async fn get_download_token(&self, token: &str, avatar_id: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(format!("{}/api/files/download_token", self.address))
            .bearer_auth(token)
            .json(&json!({"avatar_id": avatar_id}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Get download token failed: status {}, body: {}", status, text);
        }
        let v: serde_json::Value = resp.json().await?;
        let download_token = v["token"].as_str().unwrap().to_string();
        Ok(download_token)
    }

    pub async fn download_file(&self, token: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .get(format!("{}/api/files/{}", self.address, token))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Download file failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn purge_files(&self) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/admin/storage/purge", self.address))
            .header("X-Admin-Token", "test_admin")
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Purge files failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn create_sticker_pack(&self, token: &str, title: &str, short_name: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(format!("{}/api/sticker_packs", self.address))
            .bearer_auth(token)
            .json(&json!({"title": title, "short_name": short_name}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Create sticker pack failed: status {}, body: {}", status, text);
        }
        let pack: serde_json::Value = resp.json().await?;
        let pack_id = pack["id"].as_str().unwrap().to_string();
        Ok(pack_id)
    }

    pub async fn add_sticker_to_pack(&self, token: &str, pack_id: &str, file_id: &str, emoji: Option<&str>) -> anyhow::Result<String> {
        let mut body = json!({"file_id": file_id});
        if let Some(emoji) = emoji {
            body["emoji"] = json!(emoji);
        }
        let resp = self
            .client
            .post(format!("{}/api/sticker_packs/{}/stickers", self.address, pack_id))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Add sticker to pack failed: status {}, body: {}", status, text);
        }
        let sticker: serde_json::Value = resp.json().await?;
        let sticker_id = sticker["id"].as_str().unwrap().to_string();
        Ok(sticker_id)
    }

    pub async fn install_sticker_pack(&self, token: &str, pack_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/sticker_packs/{}/install", self.address, pack_id))
            .bearer_auth(token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Install sticker pack failed: status {}, body: {}", status, text);
        }
        Ok(())
    }

    pub async fn send_sticker(&self, token: &str, chat_id: &str, sticker_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .post(format!("{}/api/chats/{}/stickers", self.address, chat_id))
            .bearer_auth(token)
            .json(&json!({"sticker_id": sticker_id}))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("Send sticker failed: status {}, body: {}", status, text);
        }
        let message = resp.json::<serde_json::Value>().await?;
        Ok(message)
    }
}
