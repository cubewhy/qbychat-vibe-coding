use super::helpers::TestApp;

#[tokio::test]
async fn send_with_attachment_and_reply_and_list() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    let token_a = app.register_user("ra").await?;
    let token_b = app.register_user("rb").await?;

    let chat_id = app.create_direct_chat(&token_a, "rb").await?;

    // upload a file
    let fid = app.upload_file(&token_a, b"data".to_vec(), "a.bin", "application/octet-stream").await?;

    // send base message
    let mid1 = app.send_message(&token_a, &chat_id, "base").await?;

    // send reply with attachment
    app.send_message_with_attachments(&token_b, &chat_id, "see file", vec![&fid], Some(&mid1)).await?;

    // verify attachment row exists
    let cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message_attachments")
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(cnt, 1);

    // list messages -> should include attachments and reply_to
    let list = app
        .client
        .get(format!("{}/api/chats/{}/messages", app.address, chat_id))
        .bearer_auth(&token_a)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let arr = list.as_array().unwrap();
    assert!(arr.len() >= 2);
    let latest = &arr[0];
    assert!(latest["attachments"].as_array().unwrap().len() == 1);
    assert_eq!(latest["reply_to"]["id"].as_str(), Some(&mid1[..]));

    Ok(())
}
