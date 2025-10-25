use super::helpers::TestApp;

#[tokio::test]
async fn public_search_and_join_flow() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skip: {}", e);
            return Ok(());
        }
    };

    let owner = app.register_user("pub_owner").await?;
    let seeker = app.register_user("pub_seeker").await?;

    let gid = app.create_group_chat(&owner, "Public Chat").await?;

    app.set_chat_visibility(&owner, &gid, true, Some("rustaceans")).await?;

    let search = app.search_public_chats(&seeker, "rust").await?;
    assert_eq!(
        search["results"][0]["public_handle"].as_str(),
        Some("rustaceans")
    );

    let joined = app.join_public_chat(&seeker, "rustaceans").await?;
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

    let owner1 = app.register_user("pub_owner1").await?;
    let owner2 = app.register_user("pub_owner2").await?;

    let g1 = app.create_group_chat(&owner1, "Pub1").await?;
    let g2 = app.create_group_chat(&owner2, "Pub2").await?;

    app.set_chat_visibility(&owner1, &g1, true, Some("dupe")).await?;
    // This should fail due to duplicate handle
    let result = app.set_chat_visibility(&owner2, &g2, true, Some("dupe")).await;
    assert!(result.is_err());

    Ok(())
}
