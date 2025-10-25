use actix_web::{delete, get, patch, post, web, HttpResponse};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::types::Uuid;
use sqlx::{Pool, Postgres};
use tracing::{info, instrument};

use crate::auth::{internal_err, AuthUser};
use crate::models::{
    AddParticipantReq, AdminPermissionsPayload, AdminReq, ChatDto, CreateChannelReq, CreateDirectChatReq,
    CreateGroupReq, ForwardedChatDto, ForwardedFromDto, GifMessageDto, ListQuery,
    MessageAttachmentDto, MessageDto, MessageMentionDto, MessageReadReceiptDto, MessageReplyDto,
    MuteReq, PromoteAdminReq, SetVisibilityReq, SimpleUserDto, StickerMessageDto, UnmuteReq,
};
use crate::state::AppState;
use crate::ws::ServerWsMsg;

use std::collections::HashMap;

#[derive(sqlx::FromRow)]
struct ChatMeta {
    is_direct: bool,
    chat_type: Option<String>,
    owner_id: Option<Uuid>,
    pinned_message_id: Option<Uuid>,
}

async fn fetch_chat_meta(
    pool: &Pool<Postgres>,
    chat_id: Uuid,
) -> Result<ChatMeta, actix_web::Error> {
    sqlx::query_as::<_, ChatMeta>(
        "SELECT is_direct, chat_type, owner_id, pinned_message_id FROM chats WHERE id = $1",
    )
    .bind(chat_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_err)?
    .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))
}

async fn ensure_member(
    pool: &Pool<Postgres>,
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

#[derive(sqlx::FromRow, Default)]
struct AdminPermRow {
    can_change_info: bool,
    can_delete_messages: bool,
    can_invite_users: bool,
    can_pin_messages: bool,
    can_manage_members: bool,
}

async fn load_admin_perms(
    pool: &Pool<Postgres>,
    chat_id: Uuid,
    user_id: Uuid,
) -> Result<Option<AdminPermissionsPayload>, actix_web::Error> {
    let row = sqlx::query_as::<_, AdminPermRow>(
        "SELECT can_change_info, can_delete_messages, can_invite_users, can_pin_messages, can_manage_members FROM chat_admin_permissions WHERE chat_id = $1 AND user_id = $2",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_err)?;
    Ok(row.map(|r| AdminPermissionsPayload {
        can_change_info: r.can_change_info,
        can_delete_messages: r.can_delete_messages,
        can_invite_users: r.can_invite_users,
        can_pin_messages: r.can_pin_messages,
        can_manage_members: r.can_manage_members,
    }))
}

fn has_perm(
    perms: Option<&AdminPermissionsPayload>,
    f: impl Fn(&AdminPermissionsPayload) -> bool,
) -> bool {
    perms.map(f).unwrap_or(false)
}

#[post("/v1/api/chats/direct")]
#[instrument(skip(state, req, user))]
pub async fn start_direct_chat(
    state: web::Data<AppState>,
    req: web::Json<CreateDirectChatReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = ensure_direct_chat(&state.pool, user.0, req.peer_user_id)
        .await
        .map_err(internal_err)?;
    let chat_dto = build_chat_dto(&state.pool, chat_id, user.0).await?;
    Ok(HttpResponse::Created().json(chat_dto))
}

#[post("/v1/api/chats/group")]
#[instrument(skip(state, req, user))]
pub async fn create_group(
    state: web::Data<AppState>,
    req: web::Json<CreateGroupReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = create_chat_with_owner(&state.pool, &req.title, "group", user.0).await?;
    let chat_dto = build_chat_dto(&state.pool, chat_id, user.0).await?;
    Ok(HttpResponse::Created().json(chat_dto))
}

#[post("/v1/api/chats/channel")]
#[instrument(skip(state, req, user))]
pub async fn create_channel(
    state: web::Data<AppState>,
    req: web::Json<CreateChannelReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = create_chat_with_owner(&state.pool, &req.title, "channel", user.0).await?;
    let chat_dto = build_chat_dto(&state.pool, chat_id, user.0).await?;
    Ok(HttpResponse::Created().json(chat_dto))
}

async fn create_chat_with_owner(
    pool: &Pool<Postgres>,
    title: &str,
    chat_type: &str,
    owner_id: Uuid,
) -> Result<Uuid, actix_web::Error> {
    let chat_id = Uuid::new_v4();
    let mut tx = pool.begin().await.map_err(internal_err)?;
    sqlx::query("INSERT INTO chats (id, is_direct, chat_type, owner_id, title) VALUES ($1, FALSE, $2, $3, $4)")
        .bind(chat_id)
        .bind(chat_type)
        .bind(owner_id)
        .bind(title.trim())
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
    sqlx::query("INSERT INTO chat_participants (chat_id, user_id) VALUES ($1, $2)")
        .bind(chat_id)
        .bind(owner_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
    sqlx::query(
        "INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(chat_id)
    .bind(owner_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;
    Ok(chat_id)
}

#[post("/v1/api/chats/{chat_id}/participants")]
#[instrument(skip(state, req, user), fields(chat_id = %path))]
pub async fn add_participant(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    req: web::Json<AddParticipantReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    let perms = load_admin_perms(&state.pool, chat_id, user.0).await?;
    if meta.owner_id != Some(user.0) && !has_perm(perms.as_ref(), |p| p.can_invite_users) {
        return Ok(HttpResponse::Forbidden().finish());
    }

    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    sqlx::query(
        "INSERT INTO chat_participants (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(chat_id)
    .bind(req.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_err)?;
    sqlx::query(
        "INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(chat_id)
    .bind(req.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;

    Ok(HttpResponse::Ok().finish())
}

#[get("/v1/api/chats/{chat_id}/admins")]
#[instrument(skip(state, user))]
pub async fn list_admins(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state.pool, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        user_id: Uuid,
        username: String,
        can_change_info: bool,
        can_delete_messages: bool,
        can_invite_users: bool,
        can_pin_messages: bool,
        can_manage_members: bool,
        granted_at: DateTime<Utc>,
        granted_by: Option<Uuid>,
    }
    let admins: Vec<Row> = sqlx::query_as(
        "SELECT cap.user_id, u.username, cap.can_change_info, cap.can_delete_messages, cap.can_invite_users, cap.can_pin_messages, cap.can_manage_members, cap.granted_at, cap.granted_by FROM chat_admin_permissions cap JOIN users u ON u.id = cap.user_id WHERE cap.chat_id = $1 ORDER BY cap.granted_at DESC",
    )
    .bind(chat_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;

    let admins_json: Vec<serde_json::Value> = admins
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "user_id": a.user_id,
                "username": a.username,
                "permissions": {
                    "can_change_info": a.can_change_info,
                    "can_delete_messages": a.can_delete_messages,
                    "can_invite_users": a.can_invite_users,
                    "can_pin_messages": a.can_pin_messages,
                    "can_manage_members": a.can_manage_members,
                },
                "granted_at": a.granted_at,
                "granted_by": a.granted_by,
            })
        })
        .collect();

    let owner_username: Option<String> = if let Some(oid) = meta.owner_id {
        sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(oid)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal_err)?
    } else {
        None
    };

    let owner_json = meta
        .owner_id
        .map(|oid| serde_json::json!({"user_id": oid, "username": owner_username.clone()}));

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "owner": owner_json,
        "admins": admins_json
    })))
}

#[post("/v1/api/chats/{chat_id}/admins")]
#[instrument(skip(state, req, user))]
pub async fn promote_admin(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    req: web::Json<PromoteAdminReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    if meta.owner_id != Some(user.0) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let perms = req
        .permissions
        .clone()
        .ok_or_else(|| actix_web::error::ErrorUnprocessableEntity("permissions required"))?;

    if !ensure_member(&state.pool, chat_id, req.user_id).await? {
        return Ok(HttpResponse::BadRequest().body("user must join chat first"));
    }

    sqlx::query(
        "INSERT INTO chat_admin_permissions (chat_id, user_id, can_change_info, can_delete_messages, can_invite_users, can_pin_messages, can_manage_members, granted_by, granted_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8, now()) ON CONFLICT (chat_id, user_id) DO UPDATE SET can_change_info = EXCLUDED.can_change_info, can_delete_messages = EXCLUDED.can_delete_messages, can_invite_users = EXCLUDED.can_invite_users, can_pin_messages = EXCLUDED.can_pin_messages, can_manage_members = EXCLUDED.can_manage_members, granted_by = EXCLUDED.granted_by, granted_at = now()",
    )
    .bind(chat_id)
    .bind(req.user_id)
    .bind(perms.can_change_info)
    .bind(perms.can_delete_messages)
    .bind(perms.can_invite_users)
    .bind(perms.can_pin_messages)
    .bind(perms.can_manage_members)
    .bind(user.0)
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;

    let username = sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = $1")
        .bind(req.user_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?;

    info!(chat_id = %chat_id, target = %req.user_id, "granted admin permissions");
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "user_id": req.user_id,
        "username": username,
        "permissions": perms,
        "granted_by": user.0,
    })))
}

#[delete("/v1/api/chats/{chat_id}/admins")]
#[instrument(skip(state, req, user))]
pub async fn demote_admin(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    req: web::Json<AdminReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    if meta.owner_id != Some(user.0) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    sqlx::query("DELETE FROM chat_admin_permissions WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(req.user_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[delete("/v1/api/chats/{chat_id}/participants")]
#[instrument(skip(state, req, user))]
pub async fn remove_participant(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    req: web::Json<AdminReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    let perms = load_admin_perms(&state.pool, chat_id, user.0).await?;
    if meta.owner_id != Some(user.0) && !has_perm(perms.as_ref(), |p| p.can_manage_members) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    sqlx::query("DELETE FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(req.user_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
    sqlx::query("DELETE FROM chat_members WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(req.user_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;

    let username = sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = $1")
        .bind(req.user_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?;

    // Broadcast chat_action
    let participants = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM chat_participants WHERE chat_id = $1",
    )
    .bind(chat_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;
    let msg = ServerWsMsg::ChatAction {
        sequence_id: state.sequence_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        chat_id,
        action_type: "user_left".to_string(),
        data: serde_json::json!({ "user": { "id": req.user_id, "username": username } }),
    };
    for uid in participants {
        if let Some(tx) = state.clients.get(&uid) {
            let _ = tx.send(msg.clone());
        }
    }

    Ok(HttpResponse::Ok().finish())
}

#[post("/v1/api/chats/{chat_id}/actions/mute")]
#[instrument(skip(state, req, user))]
pub async fn mute_member(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    req: web::Json<MuteReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    let perms = load_admin_perms(&state.pool, chat_id, user.0).await?;
    if meta.owner_id != Some(user.0) && !has_perm(perms.as_ref(), |p| p.can_manage_members) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let until = chrono::Utc::now() + chrono::Duration::minutes(req.minutes.max(1));
    sqlx::query(
        "INSERT INTO chat_mutes (chat_id, user_id, muted_until) VALUES ($1,$2,$3) ON CONFLICT (chat_id, user_id) DO UPDATE SET muted_until = EXCLUDED.muted_until",
    )
    .bind(chat_id)
    .bind(req.user_id)
    .bind(until)
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/v1/api/chats/{chat_id}/actions/unmute")]
#[instrument(skip(state, req, user))]
pub async fn unmute_member(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    req: web::Json<UnmuteReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    let perms = load_admin_perms(&state.pool, chat_id, user.0).await?;
    if meta.owner_id != Some(user.0) && !has_perm(perms.as_ref(), |p| p.can_manage_members) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    sqlx::query("DELETE FROM chat_mutes WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(req.user_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/v1/api/chats/{chat_id}/actions/leave")]
#[instrument(skip(state, user))]
pub async fn leave_chat(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    if meta.is_direct {
        return Ok(HttpResponse::MethodNotAllowed().finish());
    }
    if meta.owner_id == Some(user.0) {
        return Ok(HttpResponse::Conflict().body("transfer ownership before leaving"));
    }
    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    sqlx::query("DELETE FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(user.0)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
    sqlx::query("DELETE FROM chat_members WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(user.0)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/v1/api/chats/{chat_id}/actions/clear_messages")]
#[instrument(skip(state, user))]
pub async fn clear_messages(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    let allowed = if meta.is_direct {
        ensure_member(&state.pool, chat_id, user.0).await?
    } else {
        let perms = load_admin_perms(&state.pool, chat_id, user.0).await?;
        meta.owner_id == Some(user.0) || has_perm(perms.as_ref(), |p| p.can_delete_messages)
    };
    if !allowed {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let res = sqlx::query("DELETE FROM messages WHERE chat_id = $1")
        .bind(chat_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": res.rows_affected() })))
}

#[derive(sqlx::FromRow)]
struct MessageRecord {
    id: Uuid,
    chat_id: Uuid,
    sender_id: Uuid,
    sender_username: String,
    content: String,
    created_at: DateTime<Utc>,
    edited_at: Option<DateTime<Utc>>,
    reply_to_message_id: Option<Uuid>,
    kind: String,
    sticker_id: Option<Uuid>,
    gif_id: Option<String>,
    gif_url: Option<String>,
    gif_preview_url: Option<String>,
    gif_provider: Option<String>,
    forward_from_chat_id: Option<Uuid>,
    forward_from_sender_id: Option<Uuid>,
}

#[get("/v1/api/chats/{chat_id}/messages")]
#[instrument(skip(state, user, q))]
pub async fn list_messages(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    q: web::Query<ListQuery>,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state.pool, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    let pinned = meta.pinned_message_id;
    let include_reads = q.include_reads.unwrap_or(false);

    let limit = q.limit.unwrap_or(50).min(200) as i64;
    let base_query = if q.before.is_some() {
        "SELECT m.id, m.chat_id, m.sender_id, u.username AS sender_username, CASE WHEN m.is_deleted THEN '' ELSE m.content END AS content, m.created_at, m.edited_at, m.reply_to_message_id, m.kind, m.sticker_id, m.gif_id, m.gif_url, m.gif_preview_url, m.gif_provider, m.forward_from_chat_id, m.forward_from_sender_id FROM messages m JOIN users u ON u.id = m.sender_id WHERE m.chat_id = $1 AND m.created_at < $2 ORDER BY m.created_at DESC LIMIT $3"
    } else {
        "SELECT m.id, m.chat_id, m.sender_id, u.username AS sender_username, CASE WHEN m.is_deleted THEN '' ELSE m.content END AS content, m.created_at, m.edited_at, m.reply_to_message_id, m.kind, m.sticker_id, m.gif_id, m.gif_url, m.gif_preview_url, m.gif_provider, m.forward_from_chat_id, m.forward_from_sender_id FROM messages m JOIN users u ON u.id = m.sender_id WHERE m.chat_id = $1 ORDER BY m.created_at DESC LIMIT $2"
    };

    let rows: Vec<MessageRecord> = if let Some(before) = q.before {
        sqlx::query_as(base_query)
            .bind(chat_id)
            .bind(before)
            .bind(limit)
            .fetch_all(&state.pool)
            .await
            .map_err(internal_err)?
    } else {
        sqlx::query_as(base_query)
            .bind(chat_id)
            .bind(limit)
            .fetch_all(&state.pool)
            .await
            .map_err(internal_err)?
    };

    let message_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    if message_ids.is_empty() {
        return Ok(HttpResponse::Ok().json(Vec::<MessageDto>::new()));
    }

    let reply_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.reply_to_message_id).collect();
    let sticker_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.sticker_id).collect();
    let forward_chat_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.forward_from_chat_id).collect();
    let forward_sender_ids: Vec<Uuid> = rows
        .iter()
        .filter_map(|r| r.forward_from_sender_id)
        .collect();

    let attachments = load_attachments(&state.pool, &message_ids).await?;
    let mentions = load_mentions(&state.pool, &message_ids).await?;
    let replies = load_replies(&state.pool, &reply_ids).await?;
    let stickers = load_stickers(&state.pool, &sticker_ids).await?;
    let forwarded_chats = load_forward_chats(&state.pool, &forward_chat_ids).await?;
    let forwarded_users = load_forward_users(&state.pool, &forward_sender_ids).await?;
    let read_map = if include_reads {
        Some(
            load_read_receipts(
                &state.pool,
                &message_ids,
                meta.is_direct,
                meta.chat_type.as_deref(),
                chat_id,
            )
            .await?,
        )
    } else {
        None
    };

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let sender = SimpleUserDto {
            id: row.sender_id,
            username: row.sender_username.clone(),
        };
        let reply = row
            .reply_to_message_id
            .and_then(|rid| replies.get(&rid).cloned());
        let sticker = row.sticker_id.and_then(|sid| stickers.get(&sid).cloned());
        let gif = match (row.gif_id.clone(), row.gif_url.clone()) {
            (Some(id), Some(url)) => Some(GifMessageDto {
                id,
                url,
                preview_url: row.gif_preview_url.clone().unwrap_or_default(),
                provider: row.gif_provider.clone().unwrap_or_default(),
            }),
            _ => None,
        };
        let forwarded = row.forward_from_chat_id.map(|cid| ForwardedFromDto {
            chat: forwarded_chats.get(&cid).cloned(),
            sender: row
                .forward_from_sender_id
                .and_then(|sid| forwarded_users.get(&sid).cloned()),
        });
        let read_receipt = read_map.as_ref().and_then(|map| map.get(&row.id).cloned());

        out.push(MessageDto {
            id: row.id,
            chat_id: row.chat_id,
            sender,
            content: row.content.clone(),
            kind: row.kind.clone(),
            created_at: row.created_at,
            edited_at: row.edited_at,
            reply_to: reply,
            attachments: attachments.get(&row.id).cloned().unwrap_or_default(),
            mentions: mentions.get(&row.id).cloned().unwrap_or_default(),
            read_receipt,
            is_pinned: Some(row.id) == pinned,
            forwarded_from: forwarded,
            sticker,
            gif,
        });
    }

    Ok(HttpResponse::Ok().json(out))
}

async fn load_attachments(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<MessageAttachmentDto>>, actix_web::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        message_id: Uuid,
        file_id: Uuid,
        content_type: Option<String>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT ma.message_id, ma.file_id, sf.content_type FROM message_attachments ma JOIN storage_files sf ON sf.id = ma.file_id WHERE ma.message_id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(internal_err)?;
    let mut map: HashMap<Uuid, Vec<MessageAttachmentDto>> = HashMap::new();
    for row in rows {
        map.entry(row.message_id)
            .or_default()
            .push(MessageAttachmentDto {
                id: row.file_id,
                content_type: row.content_type.clone(),
            });
    }
    Ok(map)
}

async fn load_mentions(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<MessageMentionDto>>, actix_web::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        message_id: Uuid,
        user_id: Uuid,
        username: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT mm.message_id, mm.user_id, u.username FROM member_mentions mm JOIN users u ON u.id = mm.user_id WHERE mm.message_id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(internal_err)?;
    let mut map: HashMap<Uuid, Vec<MessageMentionDto>> = HashMap::new();
    for row in rows {
        map.entry(row.message_id)
            .or_default()
            .push(MessageMentionDto {
                user_id: row.user_id,
                username: row.username.clone(),
            });
    }
    Ok(map)
}

async fn load_replies(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, MessageReplyDto>, actix_web::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        content: String,
        created_at: DateTime<Utc>,
        sender_id: Uuid,
        username: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT m.id, CASE WHEN m.is_deleted THEN '' ELSE m.content END as content, m.created_at, m.sender_id, u.username FROM messages m JOIN users u ON u.id = m.sender_id WHERE m.id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(internal_err)?;
    let mut map = HashMap::new();
    for row in rows {
        map.insert(
            row.id,
            MessageReplyDto {
                id: row.id,
                content: row.content.clone(),
                sender: SimpleUserDto {
                    id: row.sender_id,
                    username: row.username.clone(),
                },
                created_at: row.created_at,
            },
        );
    }
    Ok(map)
}

async fn load_stickers(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, StickerMessageDto>, actix_web::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        pack_id: Uuid,
        short_name: String,
        emoji: Option<String>,
        file_id: Uuid,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.id, s.pack_id, sp.short_name, s.emoji, s.file_id FROM stickers s JOIN sticker_packs sp ON sp.id = s.pack_id WHERE s.id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(internal_err)?;
    let mut map = HashMap::new();
    for row in rows {
        map.insert(
            row.id,
            StickerMessageDto {
                id: row.id,
                pack_id: row.pack_id,
                pack_short_name: row.short_name.clone(),
                emoji: row.emoji.clone(),
                file_id: row.file_id,
            },
        );
    }
    Ok(map)
}

async fn load_forward_chats(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, ForwardedChatDto>, actix_web::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        title: Option<String>,
    }
    let rows: Vec<Row> = sqlx::query_as("SELECT id, title FROM chats WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(internal_err)?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                ForwardedChatDto {
                    id: r.id,
                    title: r.title.clone(),
                },
            )
        })
        .collect())
}

async fn load_forward_users(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, SimpleUserDto>, actix_web::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        username: String,
    }
    let rows: Vec<Row> = sqlx::query_as("SELECT id, username FROM users WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(internal_err)?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                SimpleUserDto {
                    id: r.id,
                    username: r.username.clone(),
                },
            )
        })
        .collect())
}

async fn load_read_receipts(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
    is_direct: bool,
    chat_type: Option<&str>,
    chat_id: Uuid,
) -> Result<HashMap<Uuid, MessageReadReceiptDto>, actix_web::Error> {
    let mut map = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }

    if is_direct {
        #[derive(sqlx::FromRow)]
        struct Row {
            message_id: Uuid,
            is_read: bool,
            first_read_at: Option<DateTime<Utc>>,
        }
        let rows: Vec<Row> = sqlx::query_as("SELECT message_id, is_read, first_read_at FROM message_reads_agg WHERE message_id = ANY($1)")
            .bind(ids)
            .fetch_all(pool)
            .await
            .map_err(internal_err)?;
        for row in rows {
            map.insert(
                row.message_id,
                MessageReadReceiptDto {
                    read_count: None,
                    is_read_by_peer: Some(row.is_read),
                    last_read_at: row.first_read_at,
                },
            );
        }
        return Ok(map);
    }

    if chat_type == Some("channel") {
        #[derive(sqlx::FromRow)]
        struct Row {
            message_id: Uuid,
            views: i64,
            last_view_at: Option<DateTime<Utc>>,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT message_id, views, last_view_at FROM message_views WHERE message_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(internal_err)?;
        for row in rows {
            map.insert(
                row.message_id,
                MessageReadReceiptDto {
                    read_count: Some(row.views),
                    is_read_by_peer: None,
                    last_read_at: row.last_view_at,
                },
            );
        }
        return Ok(map);
    }

    let participant_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chat_participants WHERE chat_id = $1")
            .bind(chat_id)
            .fetch_one(pool)
            .await
            .map_err(internal_err)?;
    if participant_count <= 100 {
        #[derive(sqlx::FromRow)]
        struct Row {
            message_id: Uuid,
            count: i64,
            last_read_at: Option<DateTime<Utc>>,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT message_id, COUNT(*) as count, MAX(read_at) as last_read_at FROM message_reads_small WHERE message_id = ANY($1) GROUP BY message_id",
        )
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(internal_err)?;
        for row in rows {
            map.insert(
                row.message_id,
                MessageReadReceiptDto {
                    read_count: Some(row.count),
                    is_read_by_peer: None,
                    last_read_at: row.last_read_at,
                },
            );
        }
    } else {
        #[derive(sqlx::FromRow)]
        struct Row {
            message_id: Uuid,
            is_read: bool,
            first_read_at: Option<DateTime<Utc>>,
        }
        let rows: Vec<Row> = sqlx::query_as("SELECT message_id, is_read, first_read_at FROM message_reads_agg WHERE message_id = ANY($1)")
            .bind(ids)
            .fetch_all(pool)
            .await
            .map_err(internal_err)?;
        for row in rows {
            map.insert(
                row.message_id,
                MessageReadReceiptDto {
                    read_count: None,
                    is_read_by_peer: Some(row.is_read),
                    last_read_at: row.first_read_at,
                },
            );
        }
    }
    Ok(map)
}

pub async fn ensure_direct_chat(pool: &Pool<Postgres>, a: Uuid, b: Uuid) -> anyhow::Result<Uuid> {
    #[derive(sqlx::FromRow)]
    struct ChatIdRow {
        id: Uuid,
    }
    if let Some(row) = sqlx::query_as::<_, ChatIdRow>(
        r#"SELECT c.id
           FROM chats c
           JOIN chat_participants p1 ON p1.chat_id = c.id AND p1.user_id = $1
           JOIN chat_participants p2 ON p2.chat_id = c.id AND p2.user_id = $2
           WHERE c.is_direct = TRUE
           LIMIT 1"#,
    )
    .bind(a)
    .bind(b)
    .fetch_optional(pool)
    .await?
    {
        return Ok(row.id);
    }

    let chat_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO chats (id, is_direct) VALUES ($1, TRUE)")
        .bind(chat_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO chat_participants (chat_id, user_id) VALUES ($1, $2), ($1, $3)")
        .bind(chat_id)
        .bind(a)
        .bind(b)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(chat_id)
    .bind(a)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(chat_id)
    .bind(b)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(chat_id)
}

async fn build_chat_dto(
    pool: &Pool<Postgres>,
    chat_id: Uuid,
    user_id: Uuid,
) -> Result<ChatDto, actix_web::Error> {
    #[derive(sqlx::FromRow)]
    struct ChatRow {
        id: Uuid,
        is_direct: bool,
        chat_type: Option<String>,
        owner_id: Option<Uuid>,
        title: Option<String>,
        created_at: DateTime<Utc>,
        is_public: bool,
        public_handle: Option<String>,
        pinned_message_id: Option<Uuid>,
        description: Option<String>,
    }
    let row: ChatRow = sqlx::query_as(
        "SELECT id, is_direct, chat_type, owner_id, title, created_at, is_public, public_handle, pinned_message_id, description FROM chats WHERE id = $1",
    )
    .bind(chat_id)
    .fetch_one(pool)
    .await
    .map_err(internal_err)?;

    let member_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_participants WHERE chat_id = $1")
        .bind(chat_id)
        .fetch_one(pool)
        .await
        .map_err(internal_err)?;

    let r#type = if row.is_direct {
        "direct".to_string()
    } else {
        row.chat_type.unwrap_or("group".to_string())
    };

    let owner = if let Some(oid) = row.owner_id {
        let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(oid)
            .fetch_one(pool)
            .await
            .map_err(internal_err)?;
        Some(SimpleUserDto { id: oid, username })
    } else {
        None
    };

    let pinned_message = None; // TODO: implement loading pinned message

    Ok(ChatDto {
        id: row.id,
        r#type,
        title: row.title,
        owner,
        created_at: row.created_at,
        member_count,
        is_public: row.is_public,
        public_handle: row.public_handle,
        pinned_message,
        description: row.description,
    })
}

#[post("/v1/api/chats/{chat_id}/visibility")]
#[instrument(skip(state, req, user))]
pub async fn set_visibility(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    req: web::Json<SetVisibilityReq>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let meta = fetch_chat_meta(&state.pool, chat_id).await?;
    if meta.owner_id != Some(user.0) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    if meta.is_direct {
        return Ok(HttpResponse::BadRequest().body("direct chats cannot be public"));
    }
    let handle = req
        .public_handle
        .as_ref()
        .map(|h| h.trim().to_lowercase())
        .filter(|h| !h.is_empty());
    if req.is_public && handle.is_none() {
        return Ok(HttpResponse::BadRequest().body("public_handle required"));
    }
    if let Some(ref h) = handle {
        if !h
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Ok(HttpResponse::BadRequest().body("invalid handle"));
        }
        let exists = sqlx::query_scalar::<_, Option<i32>>(
            "SELECT 1 FROM chats WHERE public_handle = $1 AND id <> $2",
        )
        .bind(h)
        .bind(chat_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?
        .is_some();
        if exists {
            return Ok(HttpResponse::Conflict().body("handle already taken"));
        }
    }
    sqlx::query("UPDATE chats SET is_public = $1, public_handle = $2 WHERE id = $3")
        .bind(req.is_public)
        .bind(handle)
        .bind(chat_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[derive(Deserialize)]
pub struct PublicSearchQuery {
    pub handle: String,
}

#[get("/v1/api/chats/public_search")]
pub async fn public_search(
    state: web::Data<AppState>,
    q: web::Query<PublicSearchQuery>,
    _user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    #[derive(sqlx::FromRow, serde::Serialize)]
    struct Row {
        id: Uuid,
        title: Option<String>,
        public_handle: Option<String>,
        chat_type: Option<String>,
    }
    let like = format!("{}%", q.handle.to_lowercase());
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, title, public_handle, chat_type FROM chats WHERE is_public = TRUE AND public_handle ILIKE $1 ORDER BY public_handle ASC LIMIT 20",
    )
    .bind(like)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "results": rows })))
}

#[post("/v1/api/chats/public_join")]
pub async fn public_join(
    state: web::Data<AppState>,
    user: AuthUser,
    body: web::Json<crate::models::PublicJoinReq>,
) -> actix_web::Result<HttpResponse> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        is_public: bool,
        chat_type: Option<String>,
    }
    let handle = body.handle.trim().to_lowercase();
    let chat = sqlx::query_as::<_, Row>(
        "SELECT id, is_public, chat_type FROM chats WHERE public_handle = $1",
    )
    .bind(handle)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?
    .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))?;
    if !chat.is_public {
        return Ok(HttpResponse::Forbidden().body("chat is not public"));
    }
    if chat.chat_type.as_deref() == Some("channel") {
        // allow join but channel semantics same as group
    }
    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    sqlx::query(
        "INSERT INTO chat_participants (chat_id, user_id) VALUES ($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(chat.id)
    .bind(user.0)
    .execute(&mut *tx)
    .await
    .map_err(internal_err)?;
    sqlx::query(
        "INSERT INTO chat_members (chat_id, user_id) VALUES ($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(chat.id)
    .bind(user.0)
    .execute(&mut *tx)
    .await
    .map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"chat_id": chat.id})))
}

#[get("/v1/api/chats/{chat_id}/messages/search")]
pub async fn search_messages(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    q: web::Query<std::collections::HashMap<String, String>>,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state.pool, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let query = q.get("q").map_or("", |v| v).trim();
    if query.is_empty() {
        return Ok(HttpResponse::Ok().json(Vec::<MessageDto>::new()));
    }
    let limit = q.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50).min(200) as i64;
    let before = q.get("before").and_then(|v| DateTime::parse_from_rfc3339(v).ok()).map(|dt| dt.with_timezone(&Utc));

    let base_query = if before.is_some() {
        "SELECT m.id, m.chat_id, m.sender_id, u.username AS sender_username, CASE WHEN m.is_deleted THEN '' ELSE m.content END AS content, m.created_at, m.edited_at, m.reply_to_message_id, m.kind, m.sticker_id, m.gif_id, m.gif_url, m.gif_preview_url, m.gif_provider, m.forward_from_chat_id, m.forward_from_sender_id FROM messages m JOIN users u ON u.id = m.sender_id WHERE m.chat_id = $1 AND m.created_at < $2 AND m.content ILIKE $3 ORDER BY m.created_at DESC LIMIT $4"
    } else {
        "SELECT m.id, m.chat_id, m.sender_id, u.username AS sender_username, CASE WHEN m.is_deleted THEN '' ELSE m.content END AS content, m.created_at, m.edited_at, m.reply_to_message_id, m.kind, m.sticker_id, m.gif_id, m.gif_url, m.gif_preview_url, m.gif_provider, m.forward_from_chat_id, m.forward_from_sender_id FROM messages m JOIN users u ON u.id = m.sender_id WHERE m.chat_id = $1 AND m.content ILIKE $2 ORDER BY m.created_at DESC LIMIT $3"
    };

    let rows: Vec<MessageRecord> = if let Some(before) = before {
        sqlx::query_as(base_query)
            .bind(chat_id)
            .bind(before)
            .bind(format!("%{}%", query))
            .bind(limit)
            .fetch_all(&state.pool)
            .await
            .map_err(internal_err)?
    } else {
        sqlx::query_as(base_query)
            .bind(chat_id)
            .bind(format!("%{}%", query))
            .bind(limit)
            .fetch_all(&state.pool)
            .await
            .map_err(internal_err)?
    };

    let message_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    if message_ids.is_empty() {
        return Ok(HttpResponse::Ok().json(Vec::<MessageDto>::new()));
    }

    let attachments = load_attachments(&state.pool, &message_ids).await?;
    let mentions = load_mentions(&state.pool, &message_ids).await?;
    let stickers = load_stickers(&state.pool, &message_ids).await?;
    let replies = load_replies(&state.pool, &message_ids).await?;

    let mut dtos = Vec::new();
    for row in rows {
        let dto = MessageDto {
            id: row.id,
            chat_id: row.chat_id,
            sender: SimpleUserDto { id: row.sender_id, username: row.sender_username },
            content: row.content,
            kind: row.kind,
            created_at: row.created_at,
            edited_at: row.edited_at,
            reply_to: replies.get(&row.id).cloned(),
            attachments: attachments.get(&row.id).cloned().unwrap_or_default(),
            mentions: mentions.get(&row.id).cloned().unwrap_or_default(),
            read_receipt: None, // No reads for search
            is_pinned: false, // Assume not pinned for search
            forwarded_from: None, // Simplified for search
            sticker: stickers.get(&row.id).cloned(),
            gif: None, // Simplified for search
        };
        dtos.push(dto);
    }

    Ok(HttpResponse::Ok().json(dtos))
}
