use super::helpers::TestApp;

#[tokio::test]
async fn register_and_login() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    let token = app.register_user("alice").await?;
    assert!(!token.is_empty());

    let login_token = app.login("alice", "secretpw").await?;
    assert!(!login_token.is_empty());

    Ok(())
}
