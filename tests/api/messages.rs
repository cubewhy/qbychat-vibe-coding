use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn edit_and_delete_message() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    // register two users
    let token_a = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"a","password":"secretpw"}))
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
        .json(&json!({"username":"b","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // direct chat
    let chat_id = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_a)
        .json(&json!({"peer_username":"b"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // send message
    let mid = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, chat_id))
        .bearer_auth(&token_a)
        .json(&json!({"content":"hello"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // edit message
    let res = app
        .client
        .post(format!("{}/api/messages/{}/edit", app.address, mid))
        .bearer_auth(&token_a)
        .json(&json!({"content":"hello edit"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // delete message
    let res = app
        .client
        .post(format!("{}/api/messages/{}/delete", app.address, mid))
        .bearer_auth(&token_a)
        .send()
        .await?;
    assert!(res.status().is_success());

    Ok(())
}

#[tokio::test]
async fn read_bulk_behaviors() -> anyhow::Result<()> {
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
        .json(&json!({"username":"o","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let token_peer = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"p","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // group <=100
    let gid = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&token_owner)
        .json(&json!({"title":"G"}))
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
        .bearer_auth(&token_owner)
        .json(&json!({"username":"p"}))
        .send()
        .await?;

    let m1 = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, gid))
        .bearer_auth(&token_owner)
        .json(&json!({"content":"x"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // peer marks read -> small table updated
    let res = app
        .client
        .post(format!("{}/api/messages/read_bulk", app.address))
        .bearer_auth(&token_peer)
        .json(&json!({"chat_id": gid, "message_ids": [m1]}))
        .send()
        .await?;
    assert!(res.status().is_success());
    let cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message_reads_small")
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(cnt, 1);

    // DM -> aggregate is_read
    let dm_id = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_owner)
        .json(&json!({"peer_username":"p"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let dm_msg = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, dm_id))
        .bearer_auth(&token_owner)
        .json(&json!({"content":"dm"}))
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
        .post(format!("{}/api/messages/read_bulk", app.address))
        .bearer_auth(&token_peer)
        .json(&json!({"chat_id": dm_id, "message_ids": [dm_msg]}))
        .send()
        .await?;
    assert!(res.status().is_success());
    let agg: Option<bool> =
        sqlx::query_scalar("SELECT is_read FROM message_reads_agg WHERE message_id = $1")
            .bind(dm_msg)
            .fetch_optional(&app.pool)
            .await?;
    assert_eq!(agg, Some(true));

    Ok(())
}

#[tokio::test]
async fn listing_includes_receipts_and_pin_state() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let token_a = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"rca","password":"secretpw"}))
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
        .json(&json!({"username":"rcb","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let dm = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_a)
        .json(&json!({"peer_username":"rcb"}))
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
        .post(format!("{}/api/chats/{}/messages", app.address, dm))
        .bearer_auth(&token_a)
        .json(&json!({"content":"check read"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    app.client
        .post(format!("{}/api/messages/read_bulk", app.address))
        .bearer_auth(&token_b)
        .json(&json!({"chat_id": dm, "message_ids": [msg]}))
        .send()
        .await?;

    let list = app
        .client
        .get(format!(
            "{}/api/chats/{}/messages?include_reads=true",
            app.address, dm
        ))
        .bearer_auth(&token_a)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let top = &list.as_array().unwrap()[0];
    assert_eq!(top["read_receipt"]["is_read_by_peer"].as_bool(), Some(true));

    // pin message in group and ensure flag
    let group = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&token_a)
        .json(&json!({"title":"Pinable"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let gmsg = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, group))
        .bearer_auth(&token_a)
        .json(&json!({"content":"pin me"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    app.client
        .post(format!("{}/api/chats/{}/pin_message", app.address, group))
        .bearer_auth(&token_a)
        .json(&json!({"message_id": gmsg}))
        .send()
        .await?;

    let glist = app
        .client
        .get(format!("{}/api/chats/{}/messages", app.address, group))
        .bearer_auth(&token_a)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let pinned = glist
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"].as_str() == Some(&gmsg[..]))
        .unwrap();
    assert_eq!(pinned["is_pinned"].as_bool(), Some(true));

    Ok(())
}

#[tokio::test]
async fn read_details_endpoint_permissions() -> anyhow::Result<()> {
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
        .json(&json!({"username":"rdowner","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let admin = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"rdadmin","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let member = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"rdmember","password":"secretpw"}))
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
        .json(&json!({"title":"Readers"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    for uname in ["rdadmin", "rdmember"] {
        app.client
            .post(format!("{}/api/chats/{}/participants", app.address, gid))
            .bearer_auth(&owner)
            .json(&json!({"username":uname}))
            .send()
            .await?;
    }
    // grant admin delete perms
    app.client
        .post(format!("{}/api/chats/{}/admins", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"username":"rdadmin","permissions":{"can_delete_messages":true}}))
        .send()
        .await?;

    let mid = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"content":"who read me"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    app.client
        .post(format!("{}/api/messages/read_bulk", app.address))
        .bearer_auth(&member)
        .json(&json!({"chat_id": gid, "message_ids": [mid.clone()]}))
        .send()
        .await?;

    let reads = app
        .client
        .get(format!("{}/api/messages/{}/reads", app.address, mid))
        .bearer_auth(&admin)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert_eq!(reads["readers"].as_array().unwrap().len(), 1);

    let res = app
        .client
        .get(format!("{}/api/messages/{}/reads", app.address, mid))
        .bearer_auth(&member)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

    Ok(())
}
