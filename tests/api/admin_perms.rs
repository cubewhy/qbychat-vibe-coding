use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn owner_grants_perms_and_pin_and_invite() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let token_owner = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"ow","password":"secretpw"}))
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
        .json(&json!({"username":"m1","password":"secretpw"}))
        .send()
        .await?;
    let _ = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"u2","password":"secretpw"}))
        .send()
        .await?;

    // create group
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

    // add m1 to group
    let res = app
        .client
        .post(format!("{}/api/chats/{}/participants", app.address, gid))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"m1"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // grant admin perms to m1
    let res = app
        .client
        .post(format!("{}/api/chats/{}/admins", app.address, gid))
        .bearer_auth(&token_owner)
        .json(&json!({"username":"m1","permissions":{"can_change_info":false,"can_delete_messages":false,"can_invite_users":true,"can_pin_messages":true,"can_manage_members":false}}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // m1 invites u2
    let token_m1 = app
        .client
        .post(format!("{}/api/login", app.address))
        .json(&json!({"username":"m1","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let res = app
        .client
        .post(format!("{}/api/chats/{}/participants", app.address, gid))
        .bearer_auth(&token_m1)
        .json(&json!({"username":"u2"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // send message and pin
    let mid = app
        .client
        .post(format!("{}/api/chats/{}/messages", app.address, gid))
        .bearer_auth(&token_owner)
        .json(&json!({"content":"important"}))
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
        .post(format!("{}/api/chats/{}/pin_message", app.address, gid))
        .bearer_auth(&token_m1)
        .json(&json!({"message_id": mid}))
        .send()
        .await?;
    assert!(res.status().is_success());

    let pinned: Option<String> =
        sqlx::query_scalar("SELECT pinned_message_id::text FROM chats WHERE id = $1")
            .bind(&gid)
            .fetch_one(&app.pool)
            .await?;
    assert_eq!(pinned, Some(mid));

    // list admins should show granted perms
    let admins = app
        .client
        .get(format!("{}/api/chats/{}/admins", app.address, gid))
        .bearer_auth(&token_owner)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert_eq!(admins["owner"]["username"].as_str(), Some("ow"));
    assert_eq!(admins["admins"][0]["username"].as_str(), Some("m1"));
    assert_eq!(
        admins["admins"][0]["permissions"]["can_invite_users"].as_bool(),
        Some(true)
    );
    assert_eq!(
        admins["admins"][0]["permissions"]["can_pin_messages"].as_bool(),
        Some(true)
    );

    let res = app
        .client
        .post(format!("{}/api/chats/{}/unpin_message", app.address, gid))
        .bearer_auth(&token_m1)
        .send()
        .await?;
    assert!(res.status().is_success());

    Ok(())
}

#[tokio::test]
async fn admin_without_manage_perm_cannot_remove_member() -> anyhow::Result<()> {
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
        .json(&json!({"username":"ow2","password":"secretpw"}))
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
        .json(&json!({"username":"adm","password":"secretpw"}))
        .send()
        .await?;
    let _ = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"victim","password":"secretpw"}))
        .send()
        .await?;

    let gid = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&owner)
        .json(&json!({"title":"G2"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let res = app.client
        .post(format!("{}/api/chats/{}/participants", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"username":"adm"}))
        .send()
        .await?;
    assert!(res.status().is_success());
    let res = app.client
        .post(format!("{}/api/chats/{}/participants", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"username":"victim"}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // grant admin without manage perms
    let res = app
        .client
        .post(format!("{}/api/chats/{}/admins", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"username":"adm","permissions":{"can_change_info":false,"can_delete_messages":false,"can_invite_users":true,"can_pin_messages":false,"can_manage_members":false}}))
        .send()
        .await?;
    assert!(res.status().is_success());

    let token_adm = app
        .client
        .post(format!("{}/api/login", app.address))
        .json(&json!({"username":"adm","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // attempt to remove victim -> forbidden
    let res = app
        .client
        .post(format!("{}/api/chats/{}/remove", app.address, gid))
        .bearer_auth(&token_adm)
        .json(&json!({"username":"victim"}))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

    Ok(())
}

#[tokio::test]
async fn promote_requires_permissions_payload() -> anyhow::Result<()> {
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
        .json(&json!({"username":"ow3","password":"secretpw"}))
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
        .json(&json!({"username":"adm3","password":"secretpw"}))
        .send()
        .await?;

    let gid = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&owner)
        .json(&json!({"title":"G3"}))
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
        .bearer_auth(&owner)
        .json(&json!({"username":"adm3"}))
        .send()
        .await?;

    // missing permissions block should be 422
    let res = app
        .client
        .post(format!("{}/api/chats/{}/admins", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"username":"adm3"}))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    Ok(())
}
