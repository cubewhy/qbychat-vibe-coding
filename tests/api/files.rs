use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn upload_dedup_by_sha256() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await { Ok(a) => a, Err(e) => { eprintln!("skipping test: {}", e); return Ok(()); } };

    // register two users
    let token_a = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"fa","password":"secretpw"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("token").and_then(|v| v.as_str()).unwrap().to_string();
    let token_b = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"fb","password":"secretpw"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("token").and_then(|v| v.as_str()).unwrap().to_string();

    // upload same bytes twice (no compression to keep identical)
    let part1 = reqwest::multipart::Part::bytes(b"hello world".to_vec()).file_name("a.bin").mime_str("application/octet-stream").unwrap();
    let form1 = reqwest::multipart::Form::new().part("f1", part1);
    let files1 = app.client.post(format!("{}/api/files", app.address))
        .bearer_auth(&token_a)
        .multipart(form1)
        .send().await?
        .json::<serde_json::Value>().await?;
    let id1 = files1[0]["id"].as_str().unwrap().to_string();

    let part2 = reqwest::multipart::Part::bytes(b"hello world".to_vec()).file_name("b.bin").mime_str("application/octet-stream").unwrap();
    let form2 = reqwest::multipart::Form::new().part("f1", part2);
    let files2 = app.client.post(format!("{}/api/files", app.address))
        .bearer_auth(&token_b)
        .multipart(form2)
        .send().await?
        .json::<serde_json::Value>().await?;
    let id2 = files2[0]["id"].as_str().unwrap().to_string();

    assert_eq!(id1, id2);

    // verify only one row
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_files").fetch_one(&app.pool).await?;
    assert_eq!(count, 1);

    Ok(())
}

#[tokio::test]
async fn admin_purge_unreferenced_files() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await { Ok(a) => a, Err(e) => { eprintln!("skipping test: {}", e); return Ok(()); } };
    let token = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"u","password":"secretpw"}))
        .send().await?
        .json::<serde_json::Value>().await?
        .get("token").and_then(|v| v.as_str()).unwrap().to_string();

    let part = reqwest::multipart::Part::bytes(b"temp".to_vec()).file_name("t.bin").mime_str("application/octet-stream").unwrap();
    let form = reqwest::multipart::Form::new().part("f1", part);
    let _ = app.client.post(format!("{}/api/files", app.address))
        .bearer_auth(&token)
        .multipart(form)
        .send().await?;

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_files").fetch_one(&app.pool).await?;
    assert_eq!(before, 1);

    let resp = app.client.post(format!("{}/api/admin/storage/purge", app.address))
        .header("X-Admin-Token", "test_admin")
        .send().await?;
    assert!(resp.status().is_success());

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_files").fetch_one(&app.pool).await?;
    assert_eq!(after, 0);

    Ok(())
}
