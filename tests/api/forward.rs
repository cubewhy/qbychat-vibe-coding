use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn forward_messages_preserves_metadata() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let sender = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"fw_sender","password":"secretpw"}))
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
        .json(&json!({"username":"fw_peer","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let source = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&sender)
        .json(&json!({"title":"Source"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    app.client
        .post(format!("{}/api/chats/{}/participants", app.address, source))
        .bearer_auth(&sender)
        .json(&json!({"username":"fw_peer"}))
        .send()
        .await?;

    let target = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&peer)
        .json(&json!({"title":"Target"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    app.client
        .post(format!("{}/api/chats/{}/participants", app.address, target))
        .bearer_auth(&peer)
        .json(&json!({"username":"fw_sender"}))
        .send()
        .await?;

    let msg = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, source))
        .bearer_auth(&sender)
        .json(&json!({"content":"original text"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let res = app
        .client
        .post(format!(
            "{}/api/chats/{}/forward_messages",
            app.address, target
        ))
        .bearer_auth(&sender)
        .json(&json!({"from_chat_id": source, "message_ids": [msg]}))
        .send()
        .await?;
    assert!(res.status().is_success());

    let list = app
        .client
        .get(format!("{}/api/chats/{}/messages", app.address, target))
        .bearer_auth(&sender)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let latest = &list.as_array().unwrap()[0];
    assert_eq!(
        latest["forwarded_from"]["chat"]["title"].as_str(),
        Some("Source")
    );
    assert_eq!(
        latest["forwarded_from"]["sender"]["username"].as_str(),
        Some("fw_sender")
    );

    Ok(())
}

#[tokio::test]
async fn forward_bad_path_when_not_in_source() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let sender = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"fw_bad","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let outsider = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"fw_out","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let source = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&sender)
        .json(&json!({"title":"Secret"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let msg = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, source))
        .bearer_auth(&sender)
        .json(&json!({"content":"hidden"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let target = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&outsider)
        .json(&json!({"title":"Other"}))
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
        .post(format!(
            "{}/api/chats/{}/forward_messages",
            app.address, target
        ))
        .bearer_auth(&outsider)
        .json(&json!({"from_chat_id": source, "message_ids": [msg]}))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

    Ok(())
}
