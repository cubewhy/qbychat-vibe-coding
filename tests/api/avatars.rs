use super::helpers::TestApp;
use serde_json::json;

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
    let token = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"u1","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // upload two avatars
    let part1 = reqwest::multipart::Part::bytes("PNG1".as_bytes().to_vec())
        .file_name("a.png")
        .mime_str("image/png")
        .unwrap();
    let part2 = reqwest::multipart::Part::bytes("PNG2".as_bytes().to_vec())
        .file_name("b.png")
        .mime_str("image/png")
        .unwrap();
    let form = reqwest::multipart::Form::new()
        .part("file1", part1)
        .part("file2", part2);
    let res = app
        .client
        .post(format!("{}/api/users/me/avatars", app.address))
        .bearer_auth(&token)
        .multipart(form)
        .send()
        .await?;
    assert!(res.status().is_success());
    let uploaded: Vec<serde_json::Value> = res.json().await?;
    assert!(uploaded.len() >= 2);
    let id1 = uploaded[0]["id"].as_str().unwrap().to_string();

    // set primary
    let res = app
        .client
        .post(format!("{}/api/users/me/avatars/primary", app.address))
        .bearer_auth(&token)
        .json(&json!({"avatar_id": id1}))
        .send()
        .await?;
    assert!(res.status().is_success());

    // list my avatars
    let me = app
        .client
        .post(format!("{}/api/login", app.address))
        .json(&json!({"username":"u1","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let my_id = me["user"]["id"].as_str().unwrap();
    let res = app
        .client
        .get(format!("{}/api/users/{}/avatars", app.address, my_id))
        .send()
        .await?;
    assert!(res.status().is_success());
    let list: Vec<serde_json::Value> = res.json().await?;
    assert!(!list.is_empty());

    // download token
    let res = app
        .client
        .post(format!("{}/api/files/download_token", app.address))
        .bearer_auth(&token)
        .json(&json!({"avatar_id": id1}))
        .send()
        .await?;
    assert!(res.status().is_success());
    let v: serde_json::Value = res.json().await?;
    let t = v["token"].as_str().unwrap();

    // download file
    let res = app
        .client
        .get(format!("{}/api/files/{}", app.address, t))
        .send()
        .await?;
    assert!(res.status().is_success());

    Ok(())
}
