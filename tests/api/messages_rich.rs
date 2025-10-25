use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn send_with_attachment_and_reply_and_list() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    let token_a = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"ra","password":"secretpw"}))
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
        .json(&json!({"username":"rb","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let chat_id = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_a)
        .json(&json!({"peer_username":"rb"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // upload a file
    let part = reqwest::multipart::Part::bytes(b"data".to_vec())
        .file_name("a.bin")
        .mime_str("application/octet-stream")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("f1", part);
    let files = app
        .client
        .post(format!("{}/api/files", app.address))
        .bearer_auth(&token_a)
        .multipart(form)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let fid = files[0]["id"].as_str().unwrap().to_string();

    // send base message
    let m1 = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, chat_id))
        .bearer_auth(&token_a)
        .json(&json!({"content":"base"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let mid1 = m1["id"].as_str().unwrap().to_string();

    // send reply with attachment
    let m2 = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, chat_id))
        .bearer_auth(&token_b)
        .json(&json!({"content":"see file","attachment_ids":[fid],"reply_to_message_id": mid1}))
        .send()
        .await?;
    assert!(m2.status().is_success());

    // verify attachment row exists
    let cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message_attachments")
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(cnt, 1);

    // list messages -> should include attachments and reply_to
    let list = app
        .client
        .get(format!("{}/api/chats/{}/messages", app.address, chat_id))
        .bearer_auth(&token_a)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let arr = list.as_array().unwrap();
    assert!(arr.len() >= 2);
    let latest = &arr[0];
    assert!(latest["attachments"].as_array().unwrap().len() == 1);
    assert_eq!(latest["reply_to"]["id"].as_str(), Some(&mid1[..]));

    Ok(())
}
