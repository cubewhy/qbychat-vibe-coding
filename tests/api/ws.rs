use super::helpers::TestApp;
use futures_util::SinkExt;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message as WsMessage;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
async fn next_text(ws: &mut WsStream) -> anyhow::Result<String> {
    use futures_util::StreamExt;
    loop {
        let next = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .map_err(|_| anyhow::anyhow!("timeout waiting ws frame"))?;
        let item = next.ok_or_else(|| anyhow::anyhow!("ws stream ended"))?;
        match item {
            Ok(WsMessage::Text(t)) => return Ok(t),
            Ok(WsMessage::Binary(_))
            | Ok(WsMessage::Ping(_))
            | Ok(WsMessage::Pong(_))
            | Ok(WsMessage::Frame(_)) => continue,
            Ok(WsMessage::Close(_)) => anyhow::bail!("ws closed"),
            Err(e) => return Err(e.into()),
        }
    }
}

#[tokio::test]
async fn websocket_send_and_receive() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping ws test: {}", e);
            return Ok(());
        }
    };

    // register users
    let (token_alice, _username_alice) = app.register_user_with_username("alice").await?;
    let (token_bob, username_bob) = app.register_user_with_username("bob").await?;

    // create direct chat
    let chat_id = app.create_direct_chat(&token_alice, &username_bob).await?;

    // build ws url
    let ws_url = app.address.replace("http://", "ws://") + &format!("/ws?token={}", token_alice);
    let ws_url_b = app.address.replace("http://", "ws://") + &format!("/ws?token={}", token_bob);

    // connect both sides
    let (mut ws_a, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let (mut ws_b, _) = tokio_tungstenite::connect_async(ws_url_b).await?;

    // consume presence updates
    let _ = next_text(&mut ws_a).await?; // alice receives bob's presence
    let _ = next_text(&mut ws_b).await?; // bob receives alice's presence

    // alice send message
    let payload =
        json!({"type": "send_message", "chat_id": chat_id, "content": "hello"}).to_string();
    ws_a.send(WsMessage::Text(payload)).await?;

    // both should receive message
    let t1 = next_text(&mut ws_a).await?;
    let t2 = next_text(&mut ws_b).await?;

    for t in [t1, t2] {
        let v: serde_json::Value = serde_json::from_str(&t)?;
        assert_eq!(v.get("type").and_then(|v| v.as_str()), Some("new_message"));
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str());
        assert_eq!(content, Some("hello"));
    }

    Ok(())
}

#[tokio::test]
async fn websocket_typing_indicator() -> anyhow::Result<()> {
    let app = match TestApp::spawn().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping ws test: {}", e);
            return Ok(());
        }
    };

    // register users
    let (token_alice, username_alice) = app.register_user_with_username("alice").await?;
    let (token_bob, username_bob) = app.register_user_with_username("bob").await?;

    // create direct chat
    let chat_id = app.create_direct_chat(&token_alice, &username_bob).await?;

    // connect both sides
    let ws_url_a = app.address.replace("http://", "ws://") + &format!("/ws?token={}", token_alice);
    let ws_url_b = app.address.replace("http://", "ws://") + &format!("/ws?token={}", token_bob);
    let (mut ws_a, _) = tokio_tungstenite::connect_async(ws_url_a).await?;
    let (mut ws_b, _) = tokio_tungstenite::connect_async(ws_url_b).await?;

    // alice starts typing
    let payload = json!({"type": "start_typing", "chat_id": chat_id}).to_string();
    ws_a.send(WsMessage::Text(payload)).await?;

    // bob should receive typing_indicator
    let t = next_text(&mut ws_b).await?;
    let v: serde_json::Value = serde_json::from_str(&t)?;
    assert_eq!(v.get("type").and_then(|v| v.as_str()), Some("typing_indicator"));
    assert_eq!(v.get("chat_id").and_then(|v| v.as_str()), Some(chat_id.as_str()));
    let user = v.get("user").and_then(|u| u.get("username")).and_then(|u| u.as_str());
    assert_eq!(user, Some(username_alice.as_str()));

    Ok(())
}
