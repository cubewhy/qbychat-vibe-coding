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
    let token_a = app.register_user("a").await?;
    let _ = app.register_user("b").await?;

    // direct chat
    let chat_id = app.create_direct_chat(&token_a, "b").await?;

    // send message
    let mid = app.send_message(&token_a, &chat_id, "hello").await?;

    // edit message
    app.edit_message(&token_a, &mid, "hello edit").await?;

    // delete message
    app.delete_message(&token_a, &mid).await?;

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

    let token_owner = app.register_user("o").await?;
    let token_peer = app.register_user("p").await?;

    // group <=100
    let gid = app.create_group_chat(&token_owner, "G").await?;
    app.add_participants(&token_owner, &gid, vec!["p"]).await?;

    let m1 = app.send_message(&token_owner, &gid, "x").await?;

    // peer marks read -> small table updated
    app.mark_messages_read(&token_peer, &gid, vec![&m1]).await?;
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

    let (token_a, _username_a) = app.register_user_with_username("rca").await?;
    let (token_b, username_b) = app.register_user_with_username("rcb").await?;

    let dm = app.create_direct_chat(&token_a, &username_b).await?;
    let msg = app.send_message(&token_a, &dm, "check read").await?;
    app.mark_messages_read(&token_b, &dm, vec![&msg]).await?;

    let list = app.get_messages(&token_a, &dm, true).await?;
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

    let owner = app.register_user("rdowner").await?;
    let admin = app.register_user("rdadmin").await?;
    let member = app.register_user("rdmember").await?;

    let gid = app.create_group_chat(&owner, "Readers").await?;
    app.add_participants(&owner, &gid, vec!["rdadmin", "rdmember"]).await?;
    // grant admin delete perms - TODO: need to update add_admins to support permissions
    app.client
        .post(format!("{}/api/chats/{}/admins", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"username":"rdadmin","permissions":{"can_delete_messages":true}}))
        .send()
        .await?;

    let mid = app.send_message(&owner, &gid, "who read me").await?;

    app.mark_messages_read(&member, &gid, vec![&mid]).await?;

    let reads = app.get_message_reads(&admin, &mid).await?;
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
