use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn chat_list_with_options_and_notify() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping: {}", e);
            return Ok(());
        }
    };

    let token_a = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"la","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let token_b = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"lb","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // create dm and messages
    let dm = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_a)
        .json(&json!({"peer_username":"lb"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // message with mention of b
    let _ = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, dm))
        .bearer_auth(&token_a)
        .json(&json!({"content":"hi @lb"}))
        .send()
        .await?;

    // list with options
    let list = app
        .client
        .get(format!(
            "{}/api/chats?include_unread=true&include_first=true",
            app.address
        ))
        .bearer_auth(&token_b)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert!(list.as_array().unwrap().len() >= 1);
    let item = &list.as_array().unwrap()[0];
    assert!(item.get("unread").is_some());
    assert!(item.get("first_message").is_some());

    // set notify settings
    let until = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    let res = app
        .client
        .post(format!("{}/api/chats/{}/member/notify", app.address, dm))
        .bearer_auth(&token_b)
        .json(&json!({"mute_forever": false, "mute_until": until, "notify_type":"mentions_only"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // get notify settings
    let got = app
        .client
        .get(format!("{}/api/chats/{}/member/notify", app.address, dm))
        .bearer_auth(&token_b)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert_eq!(got["notify_type"].as_str(), Some("mentions_only"));

    // mentions list
    let mention_ids = app
        .client
        .get(format!("{}/api/chats/{}/member/mentions", app.address, dm))
        .bearer_auth(&token_b)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert!(mention_ids["message_ids"].as_array().unwrap().len() >= 1);

    // clear mentions
    let res = app
        .client
        .delete(format!("{}/api/chats/{}/member/mentions", app.address, dm))
        .bearer_auth(&token_b)
        .send()
        .await?;
    assert!(res.status().is_success());

    Ok(())
}
