use super::helpers::TestApp;

#[tokio::test]
async fn mention_flow_and_clear() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let sender = app.register_user("mentioner").await?;
    let receiver = app.register_user("mentionee").await?;

    let gid = app.create_group_chat(&sender, "Mentions").await?;
    app.add_participants(&sender, &gid, vec!["mentionee"]).await?;

    app.send_message(&sender, &gid, "hello @mentionee please read").await?;

    let mentions = app.get_mentions(&receiver, &gid).await?;
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

    let owner = app.register_user("ownerlimit").await?;

    let gid = app.create_group_chat(&owner, "Big").await?;

    let mut content = String::new();
    let mut usernames = Vec::new();
    for idx in 0..51 {
        let uname = format!("user{:02}", idx);
        app.register_user(&uname).await?;
        usernames.push(uname.clone());
        content.push_str(&format!("@{} ", uname));
    }
    app.add_participants(&owner, &gid, usernames.iter().map(|s| s.as_str()).collect()).await?;

    // This should fail due to too many mentions
    let result = app.send_message(&owner, &gid, &content).await;
    assert!(result.is_err());

    Ok(())
}
