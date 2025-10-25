use super::helpers::TestApp;

#[tokio::test]
async fn members_created_and_unread_counts() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    let token_a = app.register_user("ua").await?;
    let token_b = app.register_user("ub").await?;

    // DM and members rows
    let dm_id = app.create_direct_chat(&token_a, "ub").await?;

    // A sends two messages
    let m1 = app.send_message(&token_a, &dm_id, "m1").await?;
    let m2 = app.send_message(&token_a, &dm_id, "m2").await?;

    // B unread should be 2
    let unread = app.get_unread_count(&token_b, &dm_id).await?;
    assert_eq!(unread["unread"].as_i64(), Some(2));

    // B reads first message -> unread 1
    app.mark_messages_read(&token_b, &dm_id, vec![&m1]).await?;
    let unread = app.get_unread_count(&token_b, &dm_id).await?;
    assert_eq!(unread["unread"].as_i64(), Some(1));

    // A deletes second message -> unread becomes 0
    app.delete_message(&token_a, &m2).await?;
    let unread = app.get_unread_count(&token_b, &dm_id).await?;
    assert_eq!(unread["unread"].as_i64(), Some(0));

    Ok(())
}

#[tokio::test]
async fn member_note_crud_and_leave() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping: {}", e);
            return Ok(());
        }
    };
    let token_a = app.register_user("na").await?;
    let _token_b = app.register_user("nb").await?;
    let dm = app.create_direct_chat(&token_a, "nb").await?;

    // set note
    app.set_member_note(&token_a, &dm, "hello").await?;

    // get note
    let got = app.get_member_note(&token_a, &dm).await?;
    assert_eq!(got["note"].as_str(), Some("hello"));

    // delete note
    app.delete_member_note(&token_a, &dm).await?;
    let got = app.get_member_note(&token_a, &dm).await?;
    assert_eq!(got["note"], serde_json::Value::Null);

    // leave chat
    app.leave_chat(&token_a, &dm).await?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = (SELECT id FROM users WHERE username='na')")
        .bind(&dm).fetch_optional(&app.pool).await?;
    assert!(exists.is_none());

    Ok(())
}
