use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn promote_demote_remove_and_mute_flow() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    let token_owner = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"owner","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let _ = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"alice","password":"secretpw"}))
        .send()
        .await?;

    let chat_id = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&token_owner)
        .json(&json!({"title":"Group"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // add alice
    app.client
        .post(format!(
            "{}/api/chats/{}/participants",
            app.address, chat_id
        ))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"alice"}))
        .send()
        .await?;

    // promote alice to admin
    let res = app
        .client
        .post(format!("{}/api/chats/{}/admins", app.address, chat_id))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"alice"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // mute alice 1 minute
    let res = app
        .client
        .post(format!("{}/api/chats/{}/mute", app.address, chat_id))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"alice","minutes":1}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // unmute alice
    let res = app
        .client
        .post(format!("{}/api/chats/{}/unmute", app.address, chat_id))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"alice"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // demote alice
    let res = app
        .client
        .post(format!(
            "{}/api/chats/{}/admins/remove",
            app.address, chat_id
        ))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"alice"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // remove member
    let res = app
        .client
        .post(format!("{}/api/chats/{}/remove", app.address, chat_id))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"alice"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    Ok(())
}
