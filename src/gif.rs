use anyhow::{bail, Context};
use reqwest::Client;
use serde::Deserialize;

#[derive(Clone)]
pub struct GifProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    provider: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct GifSearchResult {
    pub id: String,
    pub url: String,
    pub preview_url: String,
    pub provider: String,
}

impl GifProvider {
    pub fn new(provider: &str, base_url: String, api_key: Option<String>, client: Client) -> Self {
        Self {
            client,
            base_url,
            api_key,
            provider: provider.to_string(),
        }
    }

    pub fn from_env() -> Option<Self> {
        let base = std::env::var("GIF_PROVIDER_BASE_URL").ok()?;
        let provider = std::env::var("GIF_PROVIDER").unwrap_or_else(|_| "tenor".to_string());
        let api_key = std::env::var("GIF_PROVIDER_API_KEY").ok();
        let client = Client::builder().build().ok()?;
        Some(Self::new(&provider, base, api_key, client))
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub async fn search(&self, query: &str, limit: u8) -> anyhow::Result<Vec<GifSearchResult>> {
        match self.provider.as_str() {
            "tenor" => self.search_tenor(query, limit).await,
            other => bail!("unsupported gif provider {}", other),
        }
    }

    async fn search_tenor(&self, query: &str, limit: u8) -> anyhow::Result<Vec<GifSearchResult>> {
        #[derive(Deserialize)]
        struct MediaFormat {
            url: String,
        }
        #[derive(Deserialize)]
        struct TenorResult {
            id: String,
            media_formats: std::collections::HashMap<String, MediaFormat>,
        }
        #[derive(Deserialize)]
        struct TenorResponse {
            results: Vec<TenorResult>,
        }

        let mut req = self
            .client
            .get(format!("{}/search", self.base_url.trim_end_matches('/')))
            .query(&[
                ("q", query),
                ("limit", &limit.to_string()),
                ("media_filter", "gif,tinygif"),
            ]);
        if let Some(key) = &self.api_key {
            req = req.query(&[("key", key.as_str())]);
        }
        let resp = req.send().await?.error_for_status()?;
        let parsed: TenorResponse = resp.json().await.context("invalid tenor response")?;
        let mut out = Vec::with_capacity(parsed.results.len());
        for r in parsed.results {
            let gif = r
                .media_formats
                .get("gif")
                .or_else(|| r.media_formats.get("mediumgif"));
            let preview = r
                .media_formats
                .get("tinygif")
                .or_else(|| r.media_formats.get("nanogif"))
                .or(gif);
            if let (Some(gif), Some(preview)) = (gif, preview) {
                out.push(GifSearchResult {
                    id: r.id.clone(),
                    url: gif.url.clone(),
                    preview_url: preview.url.clone(),
                    provider: self.provider.clone(),
                });
            }
        }
        Ok(out)
    }
}
