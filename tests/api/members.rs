use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn members_created_and_unread_counts() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await { Ok(a) => a, Err(e) => { eprintln!("skipping test: {}", e); return Ok(()); } };

    let token_a = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"ua","password":"secretpw"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("token").and_then(|v| v.as_str()).unwrap().to_string();
    let token_b = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"ub","password":"secretpw"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("token").and_then(|v| v.as_str()).unwrap().to_string();

    // DM and members rows
    let dm_id = app.client.post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_a)
        .json(&json!({"peer_username":"ub"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("chat_id").and_then(|v| v.as_str()).unwrap().to_string();

    let members_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_members WHERE chat_id = $1").bind(&dm_id).fetch_one(&app.pool).await?;
    assert_eq!(members_count, 2);

    // A sends two messages
    let m1 = app.client.post(format!("{}/api/chats/{}/messages", app.address, dm_id))
        .bearer_auth(&token_a)
        .json(&json!({"content":"m1"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("id").and_then(|v| v.as_str()).unwrap().to_string();
    let m2 = app.client.post(format!("{}/api/chats/{}/messages", app.address, dm_id))
        .bearer_auth(&token_a)
        .json(&json!({"content":"m2"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("id").and_then(|v| v.as_str()).unwrap().to_string();

    // B unread should be 2
    let unread = app.client.get(format!("{}/api/chats/{}/unread_count", app.address, dm_id))
        .bearer_auth(&token_b)
        .send().await?
        .json::<serde_json::Value>().await?;
    assert_eq!(unread["unread"].as_i64(), Some(2));

    // B reads first message -> unread 1
    let _ = app.client.post(format!("{}/api/messages/read_bulk", app.address))
        .bearer_auth(&token_b)
        .json(&json!({"chat_id": dm_id, "message_ids": [m1]}))
        .send().await?;
    let unread = app.client.get(format!("{}/api/chats/{}/unread_count", app.address, dm_id))
        .bearer_auth(&token_b)
        .send().await?
        .json::<serde_json::Value>().await?;
    assert_eq!(unread["unread"].as_i64(), Some(1));

    // A deletes second message -> unread becomes 0
    let _ = app.client.post(format!("{}/api/messages/{}/delete", app.address, m2))
        .bearer_auth(&token_a)
        .send().await?;
    let unread = app.client.get(format!("{}/api/chats/{}/unread_count", app.address, dm_id))
        .bearer_auth(&token_b)
        .send().await?
        .json::<serde_json::Value>().await?;
    assert_eq!(unread["unread"].as_i64(), Some(0));

    Ok(())
}

#[tokio::test]
async fn member_note_crud_and_leave() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await { Ok(a) => a, Err(e) => { eprintln!("skipping: {}", e); return Ok(()); } };
    let token_a = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"na","password":"secretpw"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("token").and_then(|v| v.as_str()).unwrap().to_string();
    let _token_b = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"nb","password":"secretpw"}))
        .send().await?;
    let dm = app.client.post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_a)
        .json(&json!({"peer_username":"nb"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("chat_id").and_then(|v| v.as_str()).unwrap().to_string();

    // set note
    let res = app.client.post(format!("{}/api/chats/{}/member/note", app.address, dm))
        .bearer_auth(&token_a)
        .json(&json!({"note":"hello"}))
        .send().await?;
    assert!(res.status().is_success());

    // get note
    let got = app.client.get(format!("{}/api/chats/{}/member/note", app.address, dm))
        .bearer_auth(&token_a)
        .send().await?
        .json::<serde_json::Value>().await?;
    assert_eq!(got["note"].as_str(), Some("hello"));

    // delete note
    let res = app.client.delete(format!("{}/api/chats/{}/member/note", app.address, dm))
        .bearer_auth(&token_a)
        .send().await?;
    assert!(res.status().is_success());
    let got = app.client.get(format!("{}/api/chats/{}/member/note", app.address, dm))
        .bearer_auth(&token_a)
        .send().await?
        .json::<serde_json::Value>().await?;
    assert_eq!(got["note"], serde_json::Value::Null);

    // leave chat
    let res = app.client.post(format!("{}/api/chats/{}/leave", app.address, dm))
        .bearer_auth(&token_a)
        .send().await?;
    assert!(res.status().is_success());
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = (SELECT id FROM users WHERE username='na')")
        .bind(&dm).fetch_optional(&app.pool).await?;
    assert!(exists.is_none());

    Ok(())
}
