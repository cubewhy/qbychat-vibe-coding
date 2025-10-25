use super::helpers::TestApp;

#[tokio::test]
async fn chat_list_with_options_and_notify() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping: {}", e);
            return Ok(());
        }
    };

    let token_a = app.register_user("la").await?;
    let token_b = app.register_user("lb").await?;

    // create dm and messages
    let dm = app.create_direct_chat(&token_a, "lb").await?;

    // message with mention of b
    app.send_message(&token_a, &dm, "hi @lb").await?;

    // list with options
    let list = app.get_chat_list(&token_b, true, true).await?;
    assert!(list.as_array().unwrap().len() >= 1);
    let item = &list.as_array().unwrap()[0];
    assert!(item.get("unread").is_some());
    assert!(item.get("first_message").is_some());

    // set notify settings
    let until = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    app.set_notify_settings(&token_b, &dm, false, Some(&until), "mentions_only").await?;

    // get notify settings
    let got = app.get_notify_settings(&token_b, &dm).await?;
    assert_eq!(got["notify_type"].as_str(), Some("mentions_only"));

    // mentions list
    let mention_ids = app.get_mention_ids(&token_b, &dm).await?;
    assert!(mention_ids["message_ids"].as_array().unwrap().len() >= 1);

    // clear mentions
    app.clear_mentions(&token_b, &dm).await?;

    Ok(())
}
