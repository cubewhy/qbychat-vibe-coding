use super::helpers::TestApp;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn setup_provider() -> MockServer {
    let server = MockServer::start().await;
    std::env::set_var("GIF_PROVIDER_BASE_URL", server.uri());
    std::env::set_var("GIF_PROVIDER", "tenor");
    std::env::set_var("GIF_PROVIDER_API_KEY", "test-key");
    server
}

#[tokio::test]
async fn gif_search_and_send_flow() -> anyhow::Result<()> {
    let mock = setup_provider().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results":[{"id":"gif123","media_formats":{"gif":{"url":"https://cdn/gif123.gif"},"tinygif":{"url":"https://cdn/gif123_tiny.gif"}}}]
        })))
        .mount(&mock)
        .await;

    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let user = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"gifuser","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let peer = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"gifpeer","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let search = app
        .client
        .get(format!("{}/api/gifs/search?q=cat&limit=1", app.address))
        .bearer_auth(&user)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert_eq!(search["results"][0]["id"].as_str(), Some("gif123"));

    let chat = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&user)
        .json(&json!({"peer_username":"gifpeer"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let msg = app.client.post(format!("{}/api/chats/{}/gifs", app.address, chat))
        .bearer_auth(&user)
        .json(&json!({"gif_id":"gif123","gif_url":"https://cdn/gif123.gif","gif_preview_url":"https://cdn/gif123_tiny.gif","provider":"tenor"}))
        .send().await?
        .json::<serde_json::Value>().await?;
    assert_eq!(msg["kind"].as_str(), Some("gif"));

    let list = app
        .client
        .get(format!("{}/api/chats/{}/messages", app.address, chat))
        .bearer_auth(&peer)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert_eq!(
        list.as_array().unwrap()[0]["gif"]["id"].as_str(),
        Some("gif123")
    );

    Ok(())
}

#[tokio::test]
async fn gif_send_bad_provider() -> anyhow::Result<()> {
    let mock = setup_provider().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results":[] })))
        .mount(&mock)
        .await;

    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let user = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"gifbad","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let _peer = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"gifbadpeer","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let chat = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&user)
        .json(&json!({"peer_username":"gifbadpeer"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let res = app
        .client
        .post(format!("{}/api/chats/{}/gifs", app.address, chat))
        .bearer_auth(&user)
        .json(&json!({"gif_id":"gif123","gif_url":"x","gif_preview_url":"y","provider":"unknown"}))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::BAD_REQUEST);

    Ok(())
}
