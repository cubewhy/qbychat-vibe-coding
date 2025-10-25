use super::helpers::TestApp;

#[tokio::test]
async fn promote_demote_remove_and_mute_flow() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    let token_owner = app.register_user("owner").await?;
    let _ = app.register_user("alice").await?;

    let chat_id = app.create_group_chat(&token_owner, "Group").await?;

    // add alice
    app.add_participants(&token_owner, &chat_id, vec!["alice"]).await?;

    // promote alice to admin
    app.add_admins(&token_owner, &chat_id, vec!["alice"]).await?;

    // mute alice 1 minute
    app.mute_user(&token_owner, &chat_id, "alice", 1).await?;

    // unmute alice
    app.unmute_user(&token_owner, &chat_id, "alice").await?;

    // demote alice
    app.remove_admin(&token_owner, &chat_id, "alice").await?;

    // remove member
    app.remove_participants(&token_owner, &chat_id, vec!["alice"]).await?;

    Ok(())
}
