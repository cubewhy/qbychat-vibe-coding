use super::helpers::TestApp;

#[tokio::test]
async fn upload_dedup_by_sha256() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };

    // register two users
    let token_a = app.register_user("fa").await?;
    let token_b = app.register_user("fb").await?;

    // upload same bytes twice (no compression to keep identical)
    let id1 = app.upload_file(&token_a, b"hello world".to_vec(), "a.bin", "application/octet-stream").await?;
    let id2 = app.upload_file(&token_b, b"hello world".to_vec(), "b.bin", "application/octet-stream").await?;

    assert_eq!(id1, id2);

    // verify only one row
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_files")
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(count, 1);

    Ok(())
}

#[tokio::test]
async fn admin_purge_unreferenced_files() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping test: {}", e);
            return Ok(());
        }
    };
    let token = app.register_user("u").await?;

    let _ = app.upload_file(&token, b"temp".to_vec(), "t.bin", "application/octet-stream").await?;

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_files")
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(before, 1);

    app.purge_files().await?;

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_files")
        .fetch_one(&app.pool)
        .await?;
    assert_eq!(after, 0);

    Ok(())
}
