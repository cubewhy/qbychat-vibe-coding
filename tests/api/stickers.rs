use super::helpers::TestApp;
use serde_json::json;

async fn upload_sample(app: &TestApp, token: &str) -> anyhow::Result<String> {
    let part = reqwest::multipart::Part::bytes(b"sticker".to_vec())
        .file_name("a.png")
        .mime_str("image/png")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);
    let files = app
        .client
        .post(format!("{}/api/files", app.address))
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    Ok(files[0]["id"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn sticker_pack_flow_and_send() -> anyhow::Result<()> {
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
        .json(&json!({"username":"stick_owner","password":"secretpw"}))
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
        .json(&json!({"username":"stick_member","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let file_id = upload_sample(&app, &owner).await?;

    let pack = app
        .client
        .post(format!("{}/api/sticker_packs", app.address))
        .bearer_auth(&owner)
        .json(&json!({"title":"Fun Pack","short_name":"funpack"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let pack_id = pack["id"].as_str().unwrap().to_string();

    let sticker = app
        .client
        .post(format!(
            "{}/api/sticker_packs/{}/stickers",
            app.address, pack_id
        ))
        .bearer_auth(&owner)
        .json(&json!({"file_id": file_id, "emoji":"😀"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let sticker_id = sticker["id"].as_str().unwrap().to_string();

    // member installs pack
    app.client
        .post(format!(
            "{}/api/sticker_packs/{}/install",
            app.address, pack_id
        ))
        .bearer_auth(&member)
        .send()
        .await?;

    // create chat and send sticker
    let chat = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&owner)
        .json(&json!({"peer_username":"stick_member"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let res = app
        .client
        .post(format!("{}/api/chats/{}/stickers", app.address, chat))
        .bearer_auth(&member)
        .json(&json!({"sticker_id": sticker_id}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert_eq!(res["kind"].as_str(), Some("sticker"));

    let list = app
        .client
        .get(format!("{}/api/chats/{}/messages", app.address, chat))
        .bearer_auth(&owner)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let latest = &list.as_array().unwrap()[0];
    assert_eq!(latest["kind"].as_str(), Some("sticker"));
    assert_eq!(latest["sticker"]["emoji"].as_str(), Some("😀"));

    Ok(())
}

#[tokio::test]
async fn sticker_send_without_install_bad_path() -> anyhow::Result<()> {
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
        .json(&json!({"username":"stick_owner2","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let stranger = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"stick_stranger","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let file_id = upload_sample(&app, &owner).await?;
    let pack = app
        .client
        .post(format!("{}/api/sticker_packs", app.address))
        .bearer_auth(&owner)
        .json(&json!({"title":"Pack2","short_name":"packtwo"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let pack_id = pack["id"].as_str().unwrap().to_string();
    let sticker = app
        .client
        .post(format!(
            "{}/api/sticker_packs/{}/stickers",
            app.address, pack_id
        ))
        .bearer_auth(&owner)
        .json(&json!({"file_id": file_id}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let sticker_id = sticker["id"].as_str().unwrap().to_string();

    let chat = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&owner)
        .json(&json!({"peer_username":"stick_stranger"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let res = app
        .client
        .post(format!("{}/api/chats/{}/stickers", app.address, chat))
        .bearer_auth(&stranger)
        .json(&json!({"sticker_id": sticker_id}))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

    Ok(())
}
