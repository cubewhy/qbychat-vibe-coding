use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn dm_user_can_clear_all_messages() -> anyhow::Result<()> {
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
        .json(&json!({"username":"ca","password":"secretpw"}))
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
        .json(&json!({"username":"cb","password":"secretpw"}))
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
        .json(&json!({"peer_username":"cb"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // send messages
    for _ in 0..3 {
        let _ = app
            .client
            .post(format!("{}/api/chats/{}/messages", app.address, dm))
            .bearer_auth(&token_a)
            .json(&json!({"content":"hi"}))
            .send()
            .await?;
    }
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE chat_id = $1")
        .bind(&dm)
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(before, 3);

    // clear as B
    let res = app
        .client
        .post(format!("{}/api/chats/{}/clear_messages", app.address, dm))
        .bearer_auth(&token_b)
        .send()
        .await?;
    assert!(res.status().is_success());
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE chat_id = $1")
        .bind(&dm)
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(after, 0);

    Ok(())
}

#[tokio::test]
async fn group_clear_only_owner_or_admin() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping: {}", e);
            return Ok(());
        }
    };
    let token_owner = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"go","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let token_member = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"gm","password":"secretpw"}))
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
        .json(&json!({"username":"gm"}))
        .send()
        .await?;

    // some messages
    for _ in 0..2 {
        let _ = app
            .client
            .post(format!("{}/api/chats/{}/messages", app.address, gid))
            .bearer_auth(&token_owner)
            .json(&json!({"content":"hi"}))
            .send()
            .await?;
    }

    // member cannot clear
    let res = app
        .client
        .post(format!("{}/api/chats/{}/clear_messages", app.address, gid))
        .bearer_auth(&token_member)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

    // owner can clear
    let res = app
        .client
        .post(format!("{}/api/chats/{}/clear_messages", app.address, gid))
        .bearer_auth(&token_owner)
        .send()
        .await?;
    assert!(res.status().is_success());
    let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE chat_id = $1")
        .bind(&gid)
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(c, 0);

    Ok(())
}
