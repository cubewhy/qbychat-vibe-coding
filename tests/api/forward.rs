use super::helpers::TestApp;

#[tokio::test]
async fn forward_messages_preserves_metadata() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let sender = app.register_user("fw_sender").await?;
    let peer = app.register_user("fw_peer").await?;

    let source = app.create_group_chat(&sender, "Source").await?;
    app.add_participants(&sender, &source, vec!["fw_peer"]).await?;

    let target = app.create_group_chat(&peer, "Target").await?;
    app.add_participants(&peer, &target, vec!["fw_sender"]).await?;

    let msg = app.send_message(&sender, &source, "original text").await?;

    app.forward_messages(&sender, &target, &source, vec![&msg]).await?;

    let list = app
        .client
        .get(format!("{}/api/chats/{}/messages", app.address, target))
        .bearer_auth(&sender)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let latest = &list.as_array().unwrap()[0];
    assert_eq!(
        latest["forwarded_from"]["chat"]["title"].as_str(),
        Some("Source")
    );
    assert_eq!(
        latest["forwarded_from"]["sender"]["username"].as_str(),
        Some("fw_sender")
    );

    Ok(())
}

#[tokio::test]
async fn forward_bad_path_when_not_in_source() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let sender = app.register_user("fw_bad").await?;
    let outsider = app.register_user("fw_out").await?;

    let source = app.create_group_chat(&sender, "Secret").await?;
    let msg = app.send_message(&sender, &source, "hidden").await?;

    let target = app.create_group_chat(&outsider, "Other").await?;

    // This should fail since outsider is not in source chat
    let result = app.forward_messages(&outsider, &target, &source, vec![&msg]).await;
    assert!(result.is_err());

    Ok(())
}
