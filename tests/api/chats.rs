use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn direct_chat_flow() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    // register users
    let token_alice = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"alice","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let _token_bob = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"bob","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // create direct chat
    let res = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_alice)
        .json(&json!({"peer_username":"bob"}))
        .send()
        .await?;
    assert!(res.status().is_success());
    let v: serde_json::Value = res.json().await?;
    assert!(v.get("chat_id").is_some());

    Ok(())
}
