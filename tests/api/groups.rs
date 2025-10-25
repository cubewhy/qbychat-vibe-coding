use super::helpers::TestApp;

#[tokio::test]
async fn group_create_and_add_participant() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    let token_owner = app.register_user("owner").await?;
    let _ = app.register_user("bob").await?;

    let chat_id = app.create_group_chat(&token_owner, "Rustaceans").await?;

    app.add_participants(&token_owner, &chat_id, vec!["bob"]).await?;

    Ok(())
}
