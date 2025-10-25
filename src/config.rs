use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
    pub download: DownloadConfig,
    pub redis: RedisConfig,
    pub admin: AdminConfig,
    pub gif: GifConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub bind_addr: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub dir: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadConfig {
    pub token_ttl_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminConfig {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GifConfig {
    pub enabled: Option<bool>,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let mut builder =
            config::Config::builder().add_source(config::File::with_name("config/default"));

        if let Ok(path) = std::env::var("APP_CONFIG_FILE") {
            builder = builder.add_source(config::File::with_name(&path));
        }

        builder = builder.add_source(
            config::Environment::with_prefix("APP")
                .separator("__")
                .try_parsing(true)
                .list_separator(","),
        );

        let cfg = builder.build()?.try_deserialize::<AppConfig>()?;
        Ok(cfg.with_env_overrides()?)
    }

    fn with_env_overrides(mut self) -> anyhow::Result<Self> {
        if self.database.url.is_empty() {
            self.database.url = std::env::var("DATABASE_URL").context("DATABASE_URL not set")?;
        }
        if self.auth.jwt_secret.is_empty() {
            if let Ok(secret) = std::env::var("JWT_SECRET") {
                self.auth.jwt_secret = secret;
            }
        }
        if self.storage.dir.is_empty() {
            if let Ok(dir) = std::env::var("STORAGE_DIR") {
                self.storage.dir = dir;
            }
        }
        if let Some(url) = std::env::var("REDIS_URL").ok() {
            self.redis.url = Some(url);
        }
        if self.admin.token.is_empty() {
            if let Ok(token) = std::env::var("ADMIN_TOKEN") {
                self.admin.token = token;
            }
        }
        if self.server.bind_addr.is_empty() {
            if let Ok(addr) = std::env::var("BIND_ADDR") {
                self.server.bind_addr = addr;
            }
        }
        Ok(self)
    }
}
