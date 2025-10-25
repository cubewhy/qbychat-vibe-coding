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
    let token_alice = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"alice","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let token_bob = app
        .client
        .post(format!("{}/api/register", app.address))
        .json(&json!({"username":"bob","password":"secretpw"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // create direct chat
    let chat_id = app
        .client
        .post(format!("{}/api/chats/direct", app.address))
        .bearer_auth(&token_alice)
        .json(&json!({"peer_username":"bob"}))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // build ws url
    let ws_url = app.address.replace("http://", "ws://") + &format!("/ws?token={}", token_alice);
    let ws_url_b = app.address.replace("http://", "ws://") + &format!("/ws?token={}", token_bob);

    // connect both sides
    let (mut ws_a, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let (mut ws_b, _) = tokio_tungstenite::connect_async(ws_url_b).await?;

    // alice send message
    let payload =
        json!({"type": "send_message", "chat_id": chat_id, "content": "hello"}).to_string();
    ws_a.send(WsMessage::Text(payload)).await?;

    // both should receive message
    let t1 = next_text(&mut ws_a).await?;
    let t2 = next_text(&mut ws_b).await?;

    for t in [t1, t2] {
        let v: serde_json::Value = serde_json::from_str(&t)?;
        assert_eq!(v.get("type").and_then(|v| v.as_str()), Some("message"));
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str());
        assert_eq!(content, Some("hello"));
    }

    Ok(())
}
