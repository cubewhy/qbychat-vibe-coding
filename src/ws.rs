use crate::auth::{decode_token, extract_bearer};
use crate::models::MessageRow;
use crate::state::{AppState, PresenceStatus};
use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use actix_ws::{Message, MessageStream, Session};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;
use tokio::sync::mpsc;
use tracing::{error, info};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientWsMsg {
    #[serde(rename = "send_message")]
    SendMessage { chat_id: Uuid, content: String, request_id: Option<Uuid> },
    #[serde(rename = "start_typing")]
    StartTyping { chat_id: Uuid, request_id: Option<Uuid> },
    #[serde(rename = "mark_as_read")]
    MarkAsRead { chat_id: Uuid, last_read_message_id: Uuid, request_id: Option<Uuid> },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ServerWsMsg {
    #[serde(rename = "new_message")]
    NewMessage { message: MessageRow },
    #[serde(rename = "error")]
    Error { code: String, message: String },
    #[serde(rename = "typing_indicator")]
    TypingIndicator { chat_id: Uuid, user: UserInfo },
    #[serde(rename = "message_edited")]
    MessageEdited { message: MessageRow },
    #[serde(rename = "message_deleted")]
    MessageDeleted { chat_id: Uuid, message_ids: Vec<Uuid> },
    #[serde(rename = "messages_read")]
    MessagesRead { chat_id: Uuid, reader_user_id: Uuid, last_read_message_id: Uuid, read_count: Option<i32>, is_read_by_peer: Option<bool> },
    #[serde(rename = "presence_update")]
    PresenceUpdate { user_id: Uuid, status: String, last_seen_at: Option<String> },
    #[serde(rename = "chat_action")]
    ChatAction { chat_id: Uuid, action_type: String, data: serde_json::Value },
    #[serde(rename = "ack")]
    Ack { request_id: Uuid },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
}

#[derive(sqlx::FromRow)]
struct UserIdRow {
    user_id: Uuid,
}

#[get("/ws")]
pub async fn ws_route(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Payload,
) -> actix_web::Result<impl Responder> {
    let token = req
        .uri()
        .query()
        .and_then(|q| {
            web::Query::<std::collections::HashMap<String, String>>::from_query(q)
                .ok()?
                .get("token")
                .cloned()
        })
        .or_else(|| extract_bearer(req.headers()).ok());
    let Some(token) = token else {
        return Ok(HttpResponse::Unauthorized().finish());
    };
    let user_id = decode_token(&token, &state.jwt_secret)
        .map_err(|_| actix_web::error::ErrorUnauthorized("invalid token"))?;

    let (res, session, stream) = actix_ws::handle(&req, body)?;
    let state_cloned = state.get_ref().clone();
    actix_web::rt::spawn(ws_session(state_cloned, user_id, session, stream));
    Ok(res)
}

async fn ws_session(
    state: AppState,
    user_id: Uuid,
    mut session: Session,
    mut stream: MessageStream,
) {
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerWsMsg>();
    state.clients.insert(user_id, tx.clone());
    info!(%user_id, "ws connected");

    // Set presence online
    state.presence.insert(user_id, PresenceStatus {
        status: "online".to_string(),
        last_seen_at: None,
    });

    // Broadcast presence update to users with common chats
    let common_users = get_common_chat_users(&state, user_id).await;
    let presence_msg = ServerWsMsg::PresenceUpdate {
        user_id,
        status: "online".to_string(),
        last_seen_at: None,
    };
    for uid in common_users {
        if let Some(tx) = state.clients.get(&uid) {
            let _ = tx.send(presence_msg.clone());
        }
    }

    let mut closed = false;
    while !closed {
        tokio::select! {
            Some(msg) = stream.next() => {
                match msg {
                    Ok(Message::Text(txt)) => {
                        if let Err(e) = handle_ws_text(&state, user_id, &txt).await { error!(%user_id, ?e, "handle ws text error"); }
                    }
                    Ok(Message::Close(_)) => { closed = true; }
                    Ok(Message::Ping(data)) => { let _ = session.pong(&data); }
                    Ok(Message::Pong(_)) => {}
                    Ok(Message::Binary(_)) => {}
                    Ok(Message::Continuation(_)) | Ok(Message::Nop) => {}
                    Err(e) => { error!(?e, "ws error"); closed = true; }
                }
            }
            Some(server_msg) = rx.recv() => {
                let txt = serde_json::to_string(&server_msg).unwrap_or_else(|_| "{\"type\":\"error\",\"message\":\"serialization\"}".into());
                if session.text(txt).await.is_err() { closed = true; }
            }
            else => { closed = true; }
        }
    }

    state.clients.remove(&user_id);

    // Set presence offline
    let now = chrono::Utc::now();
    state.presence.insert(user_id, PresenceStatus {
        status: "offline".to_string(),
        last_seen_at: Some(now),
    });

    // Broadcast presence update
    let common_users = get_common_chat_users(&state, user_id).await;
    let presence_msg = ServerWsMsg::PresenceUpdate {
        user_id,
        status: "offline".to_string(),
        last_seen_at: Some(now.to_rfc3339()),
    };
    for uid in common_users {
        if let Some(tx) = state.clients.get(&uid) {
            let _ = tx.send(presence_msg.clone());
        }
    }

    info!(%user_id, "ws disconnected");
}

async fn handle_ws_text(state: &AppState, user_id: Uuid, txt: &str) -> anyhow::Result<()> {
    let msg: ClientWsMsg = serde_json::from_str(txt)?;
    match msg {
        ClientWsMsg::SendMessage { chat_id, content, request_id } => {
            let is_member =
                sqlx::query("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
                    .bind(chat_id)
                    .bind(user_id)
                    .fetch_optional(&state.pool)
                    .await?;
            if is_member.is_none() {
                send_ws_err(state, user_id, "forbidden", "not a member");
                return Ok(());
            }

            // check chat type for permissions
            #[derive(sqlx::FromRow)]
            struct Meta {
                chat_type: String,
                owner_id: Option<Uuid>,
            }
            let meta =
                sqlx::query_as::<_, Meta>("SELECT chat_type, owner_id FROM chats WHERE id = $1")
                    .bind(chat_id)
                    .fetch_optional(&state.pool)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("chat not found"))?;
            if meta.chat_type == "channel" && meta.owner_id != Some(user_id) {
                send_ws_err(state, user_id, "forbidden", "only owner can send in channel");
                return Ok(());
            }

            let mid = Uuid::new_v4();
            // check mute
            #[derive(sqlx::FromRow)]
            struct MuteRow {
                muted_until: Option<chrono::DateTime<chrono::Utc>>,
            }
            if let Some(m) = sqlx::query_as::<_, MuteRow>(
                "SELECT muted_until FROM chat_mutes WHERE chat_id = $1 AND user_id = $2",
            )
            .bind(chat_id)
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?
            {
                if let Some(until) = m.muted_until {
                    if until > chrono::Utc::now() {
                        send_ws_err(state, user_id, "forbidden", "muted");
                        return Ok(());
                    }
                }
            }

            let saved = sqlx::query_as::<_, MessageRow>(
                r#"INSERT INTO messages (id, chat_id, sender_id, content)
                   VALUES ($1, $2, $3, $4)
                   RETURNING id, chat_id, sender_id, content, created_at"#,
            )
            .bind(mid)
            .bind(chat_id)
            .bind(user_id)
            .bind(content)
            .fetch_one(&state.pool)
            .await?;

            #[derive(sqlx::FromRow)]
            struct UserIdRow {
                user_id: Uuid,
            }
            let participants = sqlx::query_as::<_, UserIdRow>(
                "SELECT user_id FROM chat_participants WHERE chat_id = $1",
            )
            .bind(chat_id)
            .fetch_all(&state.pool)
            .await?;

            let server_msg = ServerWsMsg::NewMessage {
                message: saved.clone(),
            };
            for p in participants {
                if let Some(tx) = state.clients.get(&p.user_id) {
                    let _ = tx.send(server_msg.clone());
                }
            }
            if let Some(rid) = request_id {
                if let Some(tx) = state.clients.get(&user_id) {
                    let _ = tx.send(ServerWsMsg::Ack { request_id: rid });
                }
            }
        }
        ClientWsMsg::StartTyping { chat_id, request_id } => {
            // Check if member
            let is_member = sqlx::query("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
                .bind(chat_id)
                .bind(user_id)
                .fetch_optional(&state.pool)
                .await?;
            if is_member.is_none() {
                return Ok(());
            }

            // Record typing
            state.typing.insert((chat_id, user_id), tokio::time::Instant::now());

            // Get username
            #[derive(sqlx::FromRow)]
            struct UserRow {
                username: String,
            }
            let user_row = sqlx::query_as::<_, UserRow>("SELECT username FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&state.pool)
                .await?;
            let user_info = UserInfo { id: user_id, username: user_row.username };

            // Broadcast to other members
            let participants = sqlx::query_as::<_, UserIdRow>(
                "SELECT user_id FROM chat_participants WHERE chat_id = $1 AND user_id != $2",
            )
            .bind(chat_id)
            .bind(user_id)
            .fetch_all(&state.pool)
            .await?;
            let typing_msg = ServerWsMsg::TypingIndicator { chat_id, user: user_info };
            for p in participants {
                if let Some(tx) = state.clients.get(&p.user_id) {
                    let _ = tx.send(typing_msg.clone());
                }
            }
            if let Some(rid) = request_id {
                if let Some(tx) = state.clients.get(&user_id) {
                    let _ = tx.send(ServerWsMsg::Ack { request_id: rid });
                }
            }
        }
        ClientWsMsg::MarkAsRead { chat_id, last_read_message_id, request_id } => {
            // Check if member
            let is_member = sqlx::query("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
                .bind(chat_id)
                .bind(user_id)
                .fetch_optional(&state.pool)
                .await?;
            if is_member.is_none() {
                return Ok(());
            }

            // Update last_read_message_id
            sqlx::query("UPDATE chat_members SET last_read_message_id = $1 WHERE chat_id = $2 AND user_id = $3")
                .bind(last_read_message_id)
                .bind(chat_id)
                .bind(user_id)
                .execute(&state.pool)
                .await?;

            // Broadcast messages_read
            // Logic depends on chat type
            let chat_type = sqlx::query_scalar::<_, String>("SELECT chat_type FROM chats WHERE id = $1")
                .bind(chat_id)
                .fetch_one(&state.pool)
                .await?;
            if chat_type == "direct" {
                // Find peer
                let peer_id = sqlx::query_scalar::<_, Uuid>(
                    "SELECT user_id FROM chat_participants WHERE chat_id = $1 AND user_id != $2",
                )
                .bind(chat_id)
                .bind(user_id)
                .fetch_one(&state.pool)
                .await?;
                // Send to peer
                if let Some(tx) = state.clients.get(&peer_id) {
                    let _ = tx.send(ServerWsMsg::MessagesRead {
                        chat_id,
                        reader_user_id: user_id,
                        last_read_message_id,
                        read_count: None,
                        is_read_by_peer: Some(true),
                    });
                }
            } else {
                // Group/channel: send to sender of the message
                let sender_id = sqlx::query_scalar::<_, Uuid>("SELECT sender_id FROM messages WHERE id = $1")
                    .bind(last_read_message_id)
                    .fetch_one(&state.pool)
                    .await?;
                if let Some(tx) = state.clients.get(&sender_id) {
                    // Calculate read_count if small group
                    let participant_count = sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM chat_participants WHERE chat_id = $1",
                    )
                    .bind(chat_id)
                    .fetch_one(&state.pool)
                    .await?;
                    let read_count = if participant_count <= 100 {
                        Some(sqlx::query_scalar::<_, i32>(
                            "SELECT COUNT(*) FROM message_reads_small WHERE message_id = $1",
                        )
                        .bind(last_read_message_id)
                        .fetch_one(&state.pool)
                        .await?)
                    } else {
                        None
                    };
                    let _ = tx.send(ServerWsMsg::MessagesRead {
                        chat_id,
                        reader_user_id: user_id,
                        last_read_message_id,
                        read_count,
                        is_read_by_peer: None,
                    });
                }
            }
            if let Some(rid) = request_id {
                if let Some(tx) = state.clients.get(&user_id) {
                    let _ = tx.send(ServerWsMsg::Ack { request_id: rid });
                }
            }
        }
    }
    Ok(())
}

fn send_ws_err(state: &AppState, user_id: Uuid, code: &str, msg: &str) {
    if let Some(tx) = state.clients.get(&user_id) {
        let _ = tx.send(ServerWsMsg::Error {
            code: code.to_string(),
            message: msg.to_string(),
        });
    }
}

async fn get_common_chat_users(state: &AppState, user_id: Uuid) -> Vec<Uuid> {
    // Only broadcast to users in direct chats or small groups (<=100 members)
    sqlx::query_scalar(
        "SELECT DISTINCT cp2.user_id FROM chat_participants cp1
         JOIN chat_participants cp2 ON cp1.chat_id = cp2.chat_id
         JOIN chats c ON c.id = cp1.chat_id
         LEFT JOIN (SELECT chat_id, COUNT(*) as cnt FROM chat_participants GROUP BY chat_id) pc ON pc.chat_id = c.id
         WHERE cp1.user_id = $1 AND cp2.user_id != $1
         AND (c.is_direct = TRUE OR pc.cnt <= 100)",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default()
}
