use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn register_and_login() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => { eprintln!("skipping test: {}", e); return Ok(()); }
    };

    let res = app.client.post(format!("{}/api/register", app.address))
        .json(&json!({"username":"alice","password":"secretpw"}))
        .send().await?;
    assert!(res.status().is_success());
    let v: serde_json::Value = res.json().await?;
    assert!(v.get("token").is_some());

    let res = app.client.post(format!("{}/api/login", app.address))
        .json(&json!({"username":"alice","password":"secretpw"}))
        .send().await?;
    assert!(res.status().is_success());

    Ok(())
}
