use crate::auth::{decode_token, extract_bearer};
use crate::models::MessageRow;
use crate::state::AppState;
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
    SendMessage { chat_id: Uuid, content: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ServerWsMsg {
    #[serde(rename = "message")]
    Message { message: MessageRow },
    #[serde(rename = "error")]
    Error { message: String },
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
    state.clients.insert(user_id, tx);
    info!(%user_id, "ws connected");

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
    info!(%user_id, "ws disconnected");
}

async fn handle_ws_text(state: &AppState, user_id: Uuid, txt: &str) -> anyhow::Result<()> {
    let msg: ClientWsMsg = serde_json::from_str(txt)?;
    match msg {
        ClientWsMsg::SendMessage { chat_id, content } => {
            let is_member =
                sqlx::query("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
                    .bind(chat_id)
                    .bind(user_id)
                    .fetch_optional(&state.pool)
                    .await?;
            if is_member.is_none() {
                send_ws_err(state, user_id, "not a member");
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
                send_ws_err(state, user_id, "only owner can send in channel");
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
                        send_ws_err(state, user_id, "muted");
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

            let server_msg = ServerWsMsg::Message {
                message: saved.clone(),
            };
            for p in participants {
                if let Some(tx) = state.clients.get(&p.user_id) {
                    let _ = tx.send(server_msg.clone());
                }
            }
        }
    }
    Ok(())
}

fn send_ws_err(state: &AppState, user_id: Uuid, msg: &str) {
    if let Some(tx) = state.clients.get(&user_id) {
        let _ = tx.send(ServerWsMsg::Error {
            message: msg.to_string(),
        });
    }
}
