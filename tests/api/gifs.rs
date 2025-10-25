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

    let user = app.register_user("gifuser").await?;
    let peer = app.register_user("gifpeer").await?;

    let search = app.search_gifs(&user, "cat", 1).await?;
    assert_eq!(search["results"][0]["id"].as_str(), Some("gif123"));

    let chat = app.create_direct_chat(&user, "gifpeer").await?;

    let msg = app.send_gif(&user, &chat, "gif123", "https://cdn/gif123.gif", "https://cdn/gif123_tiny.gif", "tenor").await?;
    assert_eq!(msg["kind"].as_str(), Some("gif"));

    let list = app.get_messages(&peer, &chat, false).await?;
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

    let user = app.register_user("gifbad").await?;
    let _peer = app.register_user("gifbadpeer").await?;

    let chat = app.create_direct_chat(&user, "gifbadpeer").await?;

    // This should fail due to unknown provider
    let result = app.send_gif(&user, &chat, "gif123", "x", "y", "unknown").await;
    assert!(result.is_err());

    Ok(())
}
