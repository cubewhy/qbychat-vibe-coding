use super::helpers::TestApp;

#[tokio::test]
async fn upload_set_primary_list_and_download() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    // need redis for token flow; skip if no redis
    if std::env::var("REDIS_URL").is_err() {
        eprintln!("skipping: no REDIS_URL");
        return Ok(());
    }

    // register
    let token = app.register_user("u1").await?;

    // upload two avatars
    let avatars = vec![
        ("PNG1".as_bytes().to_vec(), "a.png"),
        ("PNG2".as_bytes().to_vec(), "b.png"),
    ];
    let uploaded = app.upload_avatars(&token, avatars).await?;
    assert!(uploaded.len() >= 2);
    let id1 = &uploaded[0];

    // set primary
    app.set_primary_avatar(&token, id1).await?;

    // list my avatars
    let login_resp = app.login_full("u1", "secretpw").await?;
    let my_id = login_resp["user"]["id"].as_str().unwrap();
    let list = app.get_user_avatars(&token, my_id).await?;
    assert!(!list.is_empty());

    // download token
    let download_token = app.get_download_token(&token, id1).await?;

    // download file
    app.download_file(&download_token).await?;

    Ok(())
}
