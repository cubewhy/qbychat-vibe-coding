use actix_web::{delete, get, patch, post, web, HttpResponse};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::types::Uuid;
use tracing::instrument;

use crate::auth::{internal_err, AuthUser};
use crate::models::{ForwardMessagesReq, MessageRow};
use crate::state::AppState;
use crate::ws::ServerWsMsg;

#[derive(Deserialize)]
pub struct SendMessageReq {
    pub content: String,
    pub attachment_ids: Option<Vec<Uuid>>,
    pub reply_to_message_id: Option<Uuid>,
}

#[post("/api/chats/{chat_id}/messages")]
#[instrument(skip(state, req, user))]
pub async fn send_message(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: web::Json<SendMessageReq>,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    ensure_can_send(&state, chat_id, user.0).await?;

    if let Some(rid) = req.reply_to_message_id {
        let ok = sqlx::query_scalar::<_, Option<i32>>(
            "SELECT 1 FROM messages WHERE id = $1 AND chat_id = $2",
        )
        .bind(rid)
        .bind(chat_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?
        .is_some();
        if !ok {
            return Ok(HttpResponse::BadRequest().body("invalid reply_to_message_id"));
        }
    }

    let content = req.content.trim();
    if content.is_empty() {
        return Ok(HttpResponse::BadRequest().body("content required"));
    }

    let mentions = extract_mentions(content);
    if mentions.len() > 50 {
        return Ok(HttpResponse::UnprocessableEntity().body("too many mentions"));
    }

    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO messages (id, chat_id, sender_id, content, reply_to_message_id, kind) VALUES ($1,$2,$3,$4,$5,'text')")
        .bind(id)
        .bind(chat_id)
        .bind(user.0)
        .bind(content)
        .bind(req.reply_to_message_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;

    if let Some(att) = &req.attachment_ids {
        for fid in att {
            let exists =
                sqlx::query_scalar::<_, Option<i32>>("SELECT 1 FROM storage_files WHERE id = $1")
                    .bind(fid)
                    .fetch_one(&state.pool)
                    .await
                    .map_err(internal_err)?
                    .is_some();
            if !exists {
                return Ok(HttpResponse::BadRequest().body("invalid attachment id"));
            }
            sqlx::query("INSERT INTO message_attachments (message_id, file_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
                .bind(id)
                .bind(fid)
                .execute(&state.pool)
                .await
                .map_err(internal_err)?;
        }
    }

    handle_mentions(&state, chat_id, id, &mentions, content, user.0).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({"id": id})))
}

#[derive(Deserialize)]
pub struct EditMessageReq {
    pub content: String,
}

#[patch("/api/messages/{message_id}")]
pub async fn edit_message(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: web::Json<EditMessageReq>,
) -> actix_web::Result<HttpResponse> {
    let message_id = path.into_inner();
    let res = sqlx::query("UPDATE messages SET content = $1, edited_at = now() WHERE id = $2 AND sender_id = $3 AND is_deleted = FALSE")
        .bind(req.content.trim())
        .bind(message_id)
        .bind(user.0)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    if res.rows_affected() == 0 {
        return Ok(HttpResponse::Forbidden().finish());
    }

    // Broadcast message_edited
    if let Some(updated) = sqlx::query_as::<_, MessageRow>(
        "SELECT id, chat_id, sender_id, content, created_at, edited_at FROM messages WHERE id = $1",
    )
    .bind(message_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?
    {
        let chat_id = updated.chat_id;
        let participants = sqlx::query_scalar::<_, Uuid>(
            "SELECT user_id FROM chat_participants WHERE chat_id = $1",
        )
        .bind(chat_id)
        .fetch_all(&state.pool)
        .await
        .map_err(internal_err)?;
        let msg = ServerWsMsg::MessageEdited { message: updated };
        for uid in participants {
            if let Some(tx) = state.clients.get(&uid) {
                let _ = tx.send(msg.clone());
            }
        }
    }

    Ok(HttpResponse::Ok().finish())
}

#[delete("/api/messages/{message_id}")]
pub async fn delete_message(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let message_id = path.into_inner();
    let res = sqlx::query("UPDATE messages SET is_deleted = TRUE, deleted_at = now() WHERE id = $1 AND sender_id = $2")
        .bind(message_id)
        .bind(user.0)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    if res.rows_affected() == 0 {
        return Ok(HttpResponse::Forbidden().finish());
    }

    // Broadcast message_deleted
    if let Some(chat_id) = sqlx::query_scalar::<_, Uuid>("SELECT chat_id FROM messages WHERE id = $1")
        .bind(message_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?
    {
        let participants = sqlx::query_scalar::<_, Uuid>(
            "SELECT user_id FROM chat_participants WHERE chat_id = $1",
        )
        .bind(chat_id)
        .fetch_all(&state.pool)
        .await
        .map_err(internal_err)?;
        let msg = ServerWsMsg::MessageDeleted { chat_id, message_ids: vec![message_id] };
        for uid in participants {
            if let Some(tx) = state.clients.get(&uid) {
                let _ = tx.send(msg.clone());
            }
        }
    }

    Ok(HttpResponse::Ok().finish())
}

#[derive(Deserialize)]
pub struct ReadBulkReq {
    pub chat_id: Uuid,
    pub message_ids: Vec<Uuid>,
}

#[post("/api/messages/read_bulk")]
pub async fn read_bulk(
    state: web::Data<AppState>,
    user: AuthUser,
    req: web::Json<ReadBulkReq>,
) -> actix_web::Result<HttpResponse> {
    let chat_id = req.chat_id;
    if !ensure_member(&state.pool, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }

    #[derive(sqlx::FromRow)]
    struct Meta {
        is_direct: bool,
        chat_type: Option<String>,
    }
    let meta = sqlx::query_as::<_, Meta>("SELECT is_direct, chat_type FROM chats WHERE id = $1")
        .bind(chat_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chat_participants WHERE chat_id = $1")
            .bind(chat_id)
            .fetch_one(&state.pool)
            .await
            .map_err(internal_err)?;

    let now = chrono::Utc::now();

    if meta.chat_type.as_deref() == Some("channel") {
        for mid in &req.message_ids {
            sqlx::query("INSERT INTO message_views (message_id, views, last_view_at) VALUES ($1, 1, $2) ON CONFLICT (message_id) DO UPDATE SET views = message_views.views + 1, last_view_at = EXCLUDED.last_view_at")
                .bind(mid)
                .bind(now)
                .execute(&state.pool)
                .await
                .map_err(internal_err)?;
        }
    } else if meta.is_direct || count > 100 {
        for mid in &req.message_ids {
            sqlx::query("INSERT INTO message_reads_agg (message_id, is_read, first_read_at) VALUES ($1, TRUE, $2) ON CONFLICT (message_id) DO UPDATE SET is_read = TRUE, first_read_at = COALESCE(message_reads_agg.first_read_at, EXCLUDED.first_read_at)")
                .bind(mid)
                .bind(now)
                .execute(&state.pool)
                .await
                .map_err(internal_err)?;
        }
    } else {
        for mid in &req.message_ids {
            sqlx::query("INSERT INTO message_reads_small (message_id, user_id, read_at) VALUES ($1,$2,$3) ON CONFLICT (message_id, user_id) DO UPDATE SET read_at = EXCLUDED.read_at")
                .bind(mid)
                .bind(user.0)
                .bind(now)
                .execute(&state.pool)
                .await
                .map_err(internal_err)?;
        }
    }

    if !req.message_ids.is_empty() {
        let newest: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, created_at FROM messages WHERE chat_id = $1 AND id = ANY($2) ORDER BY created_at DESC LIMIT 1",
        )
        .bind(chat_id)
        .bind(&req.message_ids)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?;
        if let Some((new_id, new_ts)) = newest {
            let current: Option<DateTime<Utc>> = sqlx::query_scalar(
                "SELECT m.created_at FROM chat_members cm LEFT JOIN messages m ON m.id = cm.last_read_message_id WHERE cm.chat_id = $1 AND cm.user_id = $2",
            )
            .bind(chat_id)
            .bind(user.0)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal_err)?;
            if current.map(|t| new_ts > t).unwrap_or(true) {
                sqlx::query("INSERT INTO chat_members (chat_id, user_id, last_read_message_id) VALUES ($1,$2,$3) ON CONFLICT (chat_id,user_id) DO UPDATE SET last_read_message_id = EXCLUDED.last_read_message_id")
                    .bind(chat_id)
                    .bind(user.0)
                    .bind(new_id)
                    .execute(&state.pool)
                    .await
                    .map_err(internal_err)?;
            }
        }
    }

    Ok(HttpResponse::Ok().finish())
}

#[post("/api/admin/reads/purge")]
pub async fn purge_reads(state: web::Data<AppState>) -> actix_web::Result<HttpResponse> {
    let threshold = chrono::Utc::now() - chrono::Duration::days(7);
    let res = sqlx::query("DELETE FROM message_reads_small WHERE read_at < $1")
        .bind(threshold)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"deleted": res.rows_affected()})))
}

#[get("/api/chats/{chat_id}/unread_count")]
pub async fn unread_count(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state.pool, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }

    #[derive(sqlx::FromRow)]
    struct Lr {
        lr_time: Option<DateTime<Utc>>,
    }
    let lr = sqlx::query_as::<_, Lr>(
        "SELECT m.created_at as lr_time FROM chat_members cm LEFT JOIN messages m ON m.id = cm.last_read_message_id WHERE cm.chat_id = $1 AND cm.user_id = $2",
    )
    .bind(chat_id)
    .bind(user.0)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?;
    let lr_time = lr.and_then(|r| r.lr_time);

    let unread: i64 = if let Some(t) = lr_time {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages msg WHERE msg.chat_id = $1 AND msg.is_deleted = FALSE AND msg.created_at > $2",
        )
        .bind(chat_id)
        .bind(t)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages msg WHERE msg.chat_id = $1 AND msg.is_deleted = FALSE",
        )
        .bind(chat_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({"unread": unread})))
}

#[post("/api/chats/{chat_id}/forward_messages")]
#[instrument(skip(state, req, user))]
pub async fn forward_messages(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: web::Json<ForwardMessagesReq>,
) -> actix_web::Result<HttpResponse> {
    let target_chat_id = path.into_inner();
    if req.message_ids.is_empty() {
        return Ok(HttpResponse::BadRequest().body("message_ids required"));
    }
    ensure_can_send(&state, target_chat_id, user.0).await?;
    if !ensure_member(&state.pool, req.from_chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }

    #[derive(sqlx::FromRow)]
    struct SourceMsg {
        id: Uuid,
        content: String,
        sender_id: Uuid,
        kind: String,
        sticker_id: Option<Uuid>,
        gif_id: Option<String>,
        gif_url: Option<String>,
        gif_preview_url: Option<String>,
        gif_provider: Option<String>,
    }
    let sources: Vec<SourceMsg> = sqlx::query_as(
        "SELECT id, CASE WHEN is_deleted THEN '' ELSE content END as content, sender_id, kind, sticker_id, gif_id, gif_url, gif_preview_url, gif_provider FROM messages WHERE chat_id = $1 AND id = ANY($2)",
    )
    .bind(req.from_chat_id)
    .bind(&req.message_ids)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;
    if sources.is_empty() {
        return Ok(HttpResponse::BadRequest().body("messages not found"));
    }

    let mut created_ids = Vec::new();
    for src in sources {
        let new_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO messages (id, chat_id, sender_id, content, kind, sticker_id, gif_id, gif_url, gif_preview_url, gif_provider, forward_from_message_id, forward_from_chat_id, forward_from_sender_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        )
        .bind(new_id)
        .bind(target_chat_id)
        .bind(user.0)
        .bind(&src.content)
        .bind(&src.kind)
        .bind(src.sticker_id)
        .bind(src.gif_id)
        .bind(src.gif_url)
        .bind(src.gif_preview_url)
        .bind(src.gif_provider)
        .bind(src.id)
        .bind(req.from_chat_id)
        .bind(src.sender_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;

        sqlx::query("INSERT INTO message_attachments (message_id, file_id) SELECT $1, file_id FROM message_attachments WHERE message_id = $2")
            .bind(new_id)
            .bind(src.id)
            .execute(&state.pool)
            .await
            .map_err(internal_err)?;
        created_ids.push(new_id);
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({"message_ids": created_ids})))
}

#[derive(Deserialize)]
pub struct ReadsQuery {
    pub limit: Option<usize>,
}

#[get("/api/messages/{message_id}/reads")]
pub async fn list_message_reads(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    q: web::Query<ReadsQuery>,
) -> actix_web::Result<HttpResponse> {
    let message_id = path.into_inner();
    #[derive(sqlx::FromRow)]
    struct MsgMeta {
        chat_id: Uuid,
        sender_id: Uuid,
    }
    let meta =
        sqlx::query_as::<_, MsgMeta>("SELECT chat_id, sender_id FROM messages WHERE id = $1")
            .bind(message_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal_err)?
            .ok_or_else(|| actix_web::error::ErrorNotFound("message not found"))?;
    let chat_meta = fetch_basic_chat_meta(&state.pool, meta.chat_id).await?;
    if chat_meta.is_direct {
        return Ok(HttpResponse::MethodNotAllowed().finish());
    }
    let can_delete = admin_can_delete(&state.pool, meta.chat_id, user.0).await?;
    if chat_meta.owner_id != Some(user.0) && meta.sender_id != user.0 && !can_delete {
        return Ok(HttpResponse::Forbidden().finish());
    }

    let participant_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chat_participants WHERE chat_id = $1")
            .bind(meta.chat_id)
            .fetch_one(&state.pool)
            .await
            .map_err(internal_err)?;
    if participant_count > 100 {
        return Ok(HttpResponse::BadRequest().body("read details available only for small groups"));
    }

    #[derive(sqlx::FromRow, serde::Serialize)]
    struct ReaderRow {
        user_id: Uuid,
        username: String,
        read_at: DateTime<Utc>,
    }
    let limit = q.limit.unwrap_or(50).min(200) as i64;
    let readers: Vec<ReaderRow> = sqlx::query_as(
        "SELECT r.user_id, u.username, r.read_at FROM message_reads_small r JOIN users u ON u.id = r.user_id WHERE r.message_id = $1 ORDER BY r.read_at DESC LIMIT $2",
    )
    .bind(message_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message_id": message_id,
        "readers": readers,
    })))
}

pub(crate) async fn ensure_can_send(
    state: &web::Data<AppState>,
    chat_id: Uuid,
    user_id: Uuid,
) -> Result<ChatSendMeta, actix_web::Error> {
    if !ensure_member(&state.pool, chat_id, user_id).await? {
        return Err(actix_web::error::ErrorForbidden("not a member"));
    }
    let meta = fetch_send_meta(&state.pool, chat_id).await?;
    if meta.chat_type.as_deref() == Some("channel") && meta.owner_id != Some(user_id) {
        return Err(actix_web::error::ErrorForbidden(
            "only owner can send in channel",
        ));
    }
    #[derive(sqlx::FromRow)]
    struct MuteRow {
        muted_until: Option<DateTime<Utc>>,
    }
    if let Some(row) = sqlx::query_as::<_, MuteRow>(
        "SELECT muted_until FROM chat_mutes WHERE chat_id = $1 AND user_id = $2",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?
    {
        if let Some(until) = row.muted_until {
            if until > chrono::Utc::now() {
                return Err(actix_web::error::ErrorForbidden("muted"));
            }
        }
    }
    Ok(meta)
}

fn extract_mentions(content: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    let mut chars = content.chars().enumerate().peekable();
    while let Some((idx, c)) = chars.next() {
        if c == '@' {
            let mut name = String::new();
            let mut j = idx + 1;
            while let Some((pos, ch)) = chars.peek() {
                if *pos != j {
                    break;
                }
                if ch.is_alphanumeric() || *ch == '_' {
                    name.push(*ch);
                    chars.next();
                    j += 1;
                } else {
                    break;
                }
            }
            if !name.is_empty() {
                mentions.push(name.to_lowercase());
            }
        }
    }
    mentions
}

async fn handle_mentions(
    state: &web::Data<AppState>,
    chat_id: Uuid,
    message_id: Uuid,
    tokens: &[String],
    content: &str,
    sender_id: Uuid,
) -> Result<(), actix_web::Error> {
    if tokens.is_empty() {
        return Ok(());
    }
    #[derive(sqlx::FromRow)]
    struct Participant {
        user_id: Uuid,
        username: String,
    }
    let participants: Vec<Participant> = sqlx::query_as(
        "SELECT u.id as user_id, u.username FROM chat_participants cp JOIN users u ON u.id = cp.user_id WHERE cp.chat_id = $1",
    )
    .bind(chat_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;
    let map: HashMap<String, Uuid> = participants
        .into_iter()
        .filter(|p| p.user_id != sender_id)
        .map(|p| (p.username.to_lowercase(), p.user_id))
        .collect();
    let excerpt: String = content.chars().take(120).collect();
    let mut inserted = HashSet::new();
    for name in tokens {
        if let Some(&uid) = map.get(name) {
            if inserted.insert(uid) {
                sqlx::query("INSERT INTO member_mentions (chat_id, user_id, message_id, excerpt) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
                    .bind(chat_id)
                    .bind(uid)
                    .bind(message_id)
                    .bind(&excerpt)
                    .execute(&state.pool)
                    .await
                    .map_err(internal_err)?;
            }
        }
    }
    Ok(())
}

#[derive(sqlx::FromRow, Clone)]
pub(crate) struct ChatSendMeta {
    chat_type: Option<String>,
    owner_id: Option<Uuid>,
}

async fn fetch_send_meta(
    pool: &sqlx::PgPool,
    chat_id: Uuid,
) -> Result<ChatSendMeta, actix_web::Error> {
    sqlx::query_as::<_, ChatSendMeta>("SELECT chat_type, owner_id FROM chats WHERE id = $1")
        .bind(chat_id)
        .fetch_optional(pool)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))
}

pub(crate) async fn ensure_member(
    pool: &sqlx::PgPool,
    chat_id: Uuid,
    user_id: Uuid,
) -> Result<bool, actix_web::Error> {
    let is_member = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(internal_err)?
    .is_some();
    Ok(is_member)
}

use std::collections::{HashMap, HashSet};

#[derive(sqlx::FromRow)]
struct BasicChatMeta {
    owner_id: Option<Uuid>,
    is_direct: bool,
}

async fn fetch_basic_chat_meta(
    pool: &sqlx::PgPool,
    chat_id: Uuid,
) -> Result<BasicChatMeta, actix_web::Error> {
    sqlx::query_as::<_, BasicChatMeta>("SELECT owner_id, is_direct FROM chats WHERE id = $1")
        .bind(chat_id)
        .fetch_optional(pool)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))
}

async fn admin_can_delete(
    pool: &sqlx::PgPool,
    chat_id: Uuid,
    user_id: Uuid,
) -> Result<bool, actix_web::Error> {
    let can: Option<bool> = sqlx::query_scalar("SELECT can_delete_messages FROM chat_admin_permissions WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(internal_err)?;
    Ok(can.unwrap_or(false))
}
