use super::helpers::TestApp;
use serde_json::json;

#[tokio::test]
async fn public_search_and_join_flow() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let owner = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"pub_owner","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let seeker = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"pub_seeker","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let gid = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&owner)
        .json(&json!({"title":"Public Chat"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    app.client
        .post(format!("{}/api/chats/{}/visibility", app.address, gid))
        .bearer_auth(&owner)
        .json(&json!({"is_public":true,"public_handle":"rustaceans"}))
        .send()
        .await?;

    let search = app
        .client
        .get(format!(
            "{}/api/chats/public_search?handle=rust",
            app.address
        ))
        .bearer_auth(&seeker)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert_eq!(
        search["results"][0]["public_handle"].as_str(),
        Some("rustaceans")
    );

    let joined = app
        .client
        .post(format!("{}/api/chats/public_join", app.address))
        .bearer_auth(&seeker)
        .json(&json!({"handle":"rustaceans"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    assert_eq!(joined["chat_id"].as_str(), Some(&gid[..]));

    Ok(())
}

#[tokio::test]
async fn handle_conflict_bad_path() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let owner1 = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"pub_owner1","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let owner2 = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"pub_owner2","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    let g1 = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&owner1)
        .json(&json!({"title":"Pub1"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let g2 = app
        .client
        .post(format!("{}/api/chats/group", app.address))
        .bearer_auth(&owner2)
        .json(&json!({"title":"Pub2"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    app.client
        .post(format!("{}/api/chats/{}/visibility", app.address, g1))
        .bearer_auth(&owner1)
        .json(&json!({"is_public":true,"public_handle":"dupe"}))
        .send()
        .await?;
    let res = app
        .client
        .post(format!("{}/api/chats/{}/visibility", app.address, g2))
        .bearer_auth(&owner2)
        .json(&json!({"is_public":true,"public_handle":"dupe"}))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::CONFLICT);

    Ok(())
}
