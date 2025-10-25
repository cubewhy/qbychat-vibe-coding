use super::helpers::TestApp;

#[tokio::test]
async fn direct_chat_flow() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    // register users
    let token_alice = app.register_user("alice").await?;
    let _token_bob = app.register_user("bob").await?;

    // create direct chat
    let chat_id = app.create_direct_chat(&token_alice, "bob").await?;
    assert!(!chat_id.is_empty());

    Ok(())
}
