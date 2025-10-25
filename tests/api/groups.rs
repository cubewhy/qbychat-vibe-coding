use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn group_create_and_add_participant() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await { Ok(a) => a, Err(e) => { eprintln!("skipping test: {}", e); return Ok(()); } };

    let token_owner = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"owner","password":"secretpw"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("token").and_then(|v| v.as_str()).unwrap().to_string();

    let _ = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"bob","password":"secretpw"}))
        .send().await?;

    let chat_id = app.client.post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&token_owner)
        .json(&json!({"title":"Rustaceans"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("chat_id").and_then(|v| v.as_str()).unwrap().to_string();

    let res = app.client.post(format!("{}/api/chats/{}/participants", app.address, chat_id))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"bob"}))
        .send().await?;
    assert!(res.status().is_success());

    Ok(())
}
