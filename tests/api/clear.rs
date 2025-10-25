use super::helpers::TestApp;

#[tokio::test]
async fn dm_user_can_clear_all_messages() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping: {}", e);
            return Ok(());
        }
    };
    let token_a = app.register_user("ca").await?;
    let token_b = app.register_user("cb").await?;

    let dm = app.create_direct_chat(&token_a, "cb").await?;

    // send messages
    for _ in 0..3 {
        app.send_message(&token_a, &dm, "hi").await?;
    }

    // clear as B
    app.clear_messages(&token_b, &dm).await?;
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
    let token_owner = app.register_user("go").await?;
    let token_member = app.register_user("gm").await?;

    let gid = app.create_group_chat(&token_owner, "G").await?;
    app.add_participants(&token_owner, &gid, vec!["gm"]).await?;

    // some messages
    for _ in 0..2 {
        app.send_message(&token_owner, &gid, "hi").await?;
    }

    // member cannot clear - this should fail
    let result = app.clear_messages(&token_member, &gid).await;
    assert!(result.is_err());

    // owner can clear
    app.clear_messages(&token_owner, &gid).await?;
    let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE chat_id = $1")
        .bind(&gid)
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(c, 0);

    Ok(())
}
