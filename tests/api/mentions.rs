use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn mention_flow_and_clear() -> anyhow::Result<()> {
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
        .json(&json!({"username":"mentioner","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let receiver = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"mentionee","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let gid = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&sender)
        .json(&json!({"title":"Mentions"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    app.client
        .post(format!("{}/api/chats/{}/participants", app.address, gid))
        .bearer_auth(&sender)
        .json(&json!({"username":"mentionee"}))
        .send()
        .await?;

    let msg = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, gid))
        .bearer_auth(&sender)
        .json(&json!({"content":"hello @mentionee please read"}))
        .send()
        .await?;
    assert!(msg.status().is_success());

    let mentions = app
        .client
        .get(format!("{}/api/chats/{}/member/mentions", app.address, gid))
        .bearer_auth(&receiver)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let arr = mentions["mentions"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["excerpt"].as_str().unwrap().contains("hello"));

    app.client
        .delete(format!("{}/api/chats/{}/member/mentions", app.address, gid))
        .bearer_auth(&receiver)
        .send()
        .await?;
    let cleared = app
        .client
        .get(format!("{}/api/chats/{}/member/mentions", app.address, gid))
        .bearer_auth(&receiver)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert!(cleared["mentions"].as_array().unwrap().is_empty());

    Ok(())
}

#[tokio::test]
async fn mention_limit_enforced_bad_path() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let owner = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"ownerlimit","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let gid = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&owner)
        .json(&json!({"title":"Big"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let mut content = String::new();
    for idx in 0..51 {
        let uname = format!("user{:02}", idx);
        app.client
            .post(format!("{}/api/register", app.address))
            .json(&json!({"username":uname,"password":"secretpw"}))
            .send()
            .await?;
        app.client
            .post(format!("{}/api/chats/{}/participants", app.address, gid))
            .bearer_auth(&owner)
            .json(&json!({"username":uname}))
            .send()
            .await?;
        content.push_str(&format!("@{} ", uname));
    }

    let res = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"content": content}))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    Ok(())
}
