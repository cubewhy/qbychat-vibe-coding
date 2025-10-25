use super::helpers::TestApp;

async fn upload_sample(app: &TestApp, token: &str) -> anyhow::Result<String> {
    app.upload_file(token, b"sticker".to_vec(), "a.png", "image/png").await
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

    let owner = app.register_user("stick_owner").await?;
    let member = app.register_user("stick_member").await?;

    let file_id = upload_sample(&app, &owner).await?;

    let pack_id = app.create_sticker_pack(&owner, "Fun Pack", "funpack").await?;
    let sticker_id = app.add_sticker_to_pack(&owner, &pack_id, &file_id, Some("😀")).await?;

    // member installs pack
    app.install_sticker_pack(&member, &pack_id).await?;

    // create chat and send sticker
    let chat = app.create_direct_chat(&owner, "stick_member").await?;

    let res = app.send_sticker(&member, &chat, &sticker_id).await?;
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

    let owner = app.register_user("stick_owner2").await?;
    let stranger = app.register_user("stick_stranger").await?;

    let file_id = upload_sample(&app, &owner).await?;
    let pack_id = app.create_sticker_pack(&owner, "Pack2", "packtwo").await?;
    let sticker_id = app.add_sticker_to_pack(&owner, &pack_id, &file_id, None).await?;

    let chat = app.create_direct_chat(&owner, "stick_stranger").await?;

    // This should fail since stranger hasn't installed the pack
    let result = app.send_sticker(&stranger, &chat, &sticker_id).await;
    assert!(result.is_err());

    Ok(())
}
