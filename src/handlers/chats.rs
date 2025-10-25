use actix_web::{get, post, web, HttpResponse};
use sqlx::{Pool, Postgres};
use sqlx::types::Uuid;
use crate::state::AppState;
use crate::auth::{AuthUser, internal_err};
use crate::models::{CreateDirectChatReq, CreateGroupReq, CreateChannelReq, AddParticipantReq, AdminReq, MuteReq, UnmuteReq, ListQuery, MessageRow};

#[post("/api/chats/direct")]
pub async fn start_direct_chat(state: web::Data<AppState>, req: web::Json<CreateDirectChatReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    #[derive(sqlx::FromRow)]
    struct IdRow { id: Uuid }
    let peer = sqlx::query_as::<_, IdRow>("SELECT id FROM users WHERE username = $1")
        .bind(req.peer_username.trim())
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("peer not found"))?;

    let chat_id = ensure_direct_chat(&state.pool, user.0, peer.id).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "chat_id": chat_id })))
}

#[post("/api/chats/group")]
pub async fn create_group(state: web::Data<AppState>, req: web::Json<CreateGroupReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = Uuid::new_v4();
    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    sqlx::query("INSERT INTO chats (id, is_direct, chat_type, owner_id, title) VALUES ($1, FALSE, 'group', $2, $3)")
        .bind(chat_id).bind(user.0).bind(&req.title)
        .execute(&mut *tx).await.map_err(internal_err)?;
    sqlx::query("INSERT INTO chat_participants (chat_id, user_id) VALUES ($1, $2)")
        .bind(chat_id).bind(user.0).execute(&mut *tx).await.map_err(internal_err)?;
    sqlx::query("INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(chat_id).bind(user.0).execute(&mut *tx).await.map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "chat_id": chat_id })))
}

#[post("/api/chats/channel")]
pub async fn create_channel(state: web::Data<AppState>, req: web::Json<CreateChannelReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = Uuid::new_v4();
    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    sqlx::query("INSERT INTO chats (id, is_direct, chat_type, owner_id, title) VALUES ($1, FALSE, 'channel', $2, $3)")
        .bind(chat_id).bind(user.0).bind(&req.title)
        .execute(&mut *tx).await.map_err(internal_err)?;
    sqlx::query("INSERT INTO chat_participants (chat_id, user_id) VALUES ($1, $2)")
        .bind(chat_id).bind(user.0).execute(&mut *tx).await.map_err(internal_err)?;
    sqlx::query("INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(chat_id).bind(user.0).execute(&mut *tx).await.map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "chat_id": chat_id })))
}

#[post("/api/chats/{chat_id}/participants")]
pub async fn add_participant(state: web::Data<AppState>, path: web::Path<Uuid>, req: web::Json<AddParticipantReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    #[derive(sqlx::FromRow)]
    struct ChatMeta { chat_type: String, owner_id: Option<Uuid> }
    let meta = sqlx::query_as::<_, ChatMeta>("SELECT chat_type, owner_id FROM chats WHERE id = $1")
        .bind(chat_id).fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))?;

    // Only owner can add participants for now
    if meta.owner_id != Some(user.0) { return Ok(HttpResponse::Forbidden().finish()); }

    #[derive(sqlx::FromRow)]
    struct IdRow { id: Uuid }
    let peer = sqlx::query_as::<_, IdRow>("SELECT id FROM users WHERE username = $1")
        .bind(req.username.trim()).fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("user not found"))?;

    sqlx::query("INSERT INTO chat_participants (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(chat_id).bind(peer.id).execute(&state.pool).await.map_err(internal_err)?;
    sqlx::query("INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(chat_id).bind(peer.id).execute(&state.pool).await.map_err(internal_err)?;

    Ok(HttpResponse::Ok().finish())
}

#[post("/api/chats/{chat_id}/admins")]
pub async fn promote_admin(state: web::Data<AppState>, path: web::Path<Uuid>, req: web::Json<AdminReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let owner = sqlx::query_scalar::<_, Option<Uuid>>("SELECT owner_id FROM chats WHERE id = $1")
        .bind(chat_id).fetch_one(&state.pool).await.map_err(internal_err)?;
    if owner != Some(user.0) { return Ok(HttpResponse::Forbidden().finish()); }
    #[derive(sqlx::FromRow)]
    struct IdRow { id: Uuid }
    let target = sqlx::query_as::<_, IdRow>("SELECT id FROM users WHERE username = $1")
        .bind(req.username.trim()).fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("user not found"))?;
    sqlx::query("INSERT INTO chat_roles (chat_id, user_id, role) VALUES ($1, $2, 'admin') ON CONFLICT DO NOTHING")
        .bind(chat_id).bind(target.id).execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/api/chats/{chat_id}/admins/remove")]
pub async fn demote_admin(state: web::Data<AppState>, path: web::Path<Uuid>, req: web::Json<AdminReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let owner = sqlx::query_scalar::<_, Option<Uuid>>("SELECT owner_id FROM chats WHERE id = $1")
        .bind(chat_id).fetch_one(&state.pool).await.map_err(internal_err)?;
    if owner != Some(user.0) { return Ok(HttpResponse::Forbidden().finish()); }
    #[derive(sqlx::FromRow)]
    struct IdRow { id: Uuid }
    let target = sqlx::query_as::<_, IdRow>("SELECT id FROM users WHERE username = $1")
        .bind(req.username.trim()).fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("user not found"))?;
    sqlx::query("DELETE FROM chat_roles WHERE chat_id = $1 AND user_id = $2 AND role = 'admin'")
        .bind(chat_id).bind(target.id).execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/api/chats/{chat_id}/remove")]
pub async fn remove_participant(state: web::Data<AppState>, path: web::Path<Uuid>, req: web::Json<AdminReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    // owner or admin can remove
    #[derive(sqlx::FromRow)]
    struct Meta { owner_id: Option<Uuid> }
    let meta = sqlx::query_as::<_, Meta>("SELECT owner_id FROM chats WHERE id = $1").bind(chat_id)
        .fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))?;
    let is_admin = sqlx::query_scalar::<_, Option<i64>>("SELECT 1 FROM chat_roles WHERE chat_id = $1 AND user_id = $2 AND role = 'admin'")
        .bind(chat_id).bind(user.0).fetch_one(&state.pool).await.map_err(internal_err)?.is_some();
    if meta.owner_id != Some(user.0) && !is_admin { return Ok(HttpResponse::Forbidden().finish()); }
    #[derive(sqlx::FromRow)]
    struct IdRow { id: Uuid }
    let target = sqlx::query_as::<_, IdRow>("SELECT id FROM users WHERE username = $1")
        .bind(req.username.trim()).fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("user not found"))?;
    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    sqlx::query("DELETE FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id).bind(target.id).execute(&mut *tx).await.map_err(internal_err)?;
    sqlx::query("DELETE FROM chat_members WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id).bind(target.id).execute(&mut *tx).await.map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/api/chats/{chat_id}/mute")]
pub async fn mute_member(state: web::Data<AppState>, path: web::Path<Uuid>, req: web::Json<MuteReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    // owner or admin can mute
    let owner = sqlx::query_scalar::<_, Option<Uuid>>("SELECT owner_id FROM chats WHERE id = $1")
        .bind(chat_id).fetch_one(&state.pool).await.map_err(internal_err)?;
    let is_admin = sqlx::query_scalar::<_, Option<i64>>("SELECT 1 FROM chat_roles WHERE chat_id = $1 AND user_id = $2 AND role = 'admin'")
        .bind(chat_id).bind(user.0).fetch_one(&state.pool).await.map_err(internal_err)?.is_some();
    if owner != Some(user.0) && !is_admin { return Ok(HttpResponse::Forbidden().finish()); }
    #[derive(sqlx::FromRow)]
    struct IdRow { id: Uuid }
    let target = sqlx::query_as::<_, IdRow>("SELECT id FROM users WHERE username = $1")
        .bind(req.username.trim()).fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("user not found"))?;
    let until = chrono::Utc::now() + chrono::Duration::minutes(req.minutes);
    sqlx::query("INSERT INTO chat_mutes (chat_id, user_id, muted_until) VALUES ($1, $2, $3) ON CONFLICT (chat_id, user_id) DO UPDATE SET muted_until = EXCLUDED.muted_until")
        .bind(chat_id).bind(target.id).bind(until).execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/api/chats/{chat_id}/unmute")]
pub async fn unmute_member(state: web::Data<AppState>, path: web::Path<Uuid>, req: web::Json<UnmuteReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let owner = sqlx::query_scalar::<_, Option<Uuid>>("SELECT owner_id FROM chats WHERE id = $1")
        .bind(chat_id).fetch_one(&state.pool).await.map_err(internal_err)?;
    let is_admin = sqlx::query_scalar::<_, Option<i64>>("SELECT 1 FROM chat_roles WHERE chat_id = $1 AND user_id = $2 AND role = 'admin'")
        .bind(chat_id).bind(user.0).fetch_one(&state.pool).await.map_err(internal_err)?.is_some();
    if owner != Some(user.0) && !is_admin { return Ok(HttpResponse::Forbidden().finish()); }
    #[derive(sqlx::FromRow)]
    struct IdRow { id: Uuid }
    let target = sqlx::query_as::<_, IdRow>("SELECT id FROM users WHERE username = $1")
        .bind(req.username.trim()).fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("user not found"))?;
    sqlx::query("DELETE FROM chat_mutes WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id).bind(target.id).execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/api/chats/{chat_id}/clear_messages")]
pub async fn clear_messages(state: web::Data<AppState>, path: web::Path<Uuid>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    #[derive(sqlx::FromRow)]
    struct Meta { is_direct: bool, chat_type: Option<String>, owner_id: Option<Uuid> }
    let meta = sqlx::query_as::<_, Meta>("SELECT is_direct, chat_type, owner_id FROM chats WHERE id = $1")
        .bind(chat_id).fetch_optional(&state.pool).await.map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))?;

    let mut allowed = false;
    if meta.is_direct {
        allowed = sqlx::query_scalar::<_, Option<i64>>("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
            .bind(chat_id).bind(user.0).fetch_one(&state.pool).await.map_err(internal_err)?.is_some();
    } else {
        let is_admin = sqlx::query_scalar::<_, Option<i64>>("SELECT 1 FROM chat_roles WHERE chat_id = $1 AND user_id = $2 AND role='admin'")
            .bind(chat_id).bind(user.0).fetch_one(&state.pool).await.map_err(internal_err)?.is_some();
        allowed = meta.owner_id == Some(user.0) || is_admin;
    }
    if !allowed { return Ok(HttpResponse::Forbidden().finish()); }

    sqlx::query("DELETE FROM messages WHERE chat_id = $1").bind(chat_id).execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[get("/api/chats/{chat_id}/messages")]
pub async fn list_messages(state: web::Data<AppState>, path: web::Path<Uuid>, user: AuthUser, q: web::Query<ListQuery>) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let membership = sqlx::query("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(user.0)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?;
    if membership.is_none() { return Ok(HttpResponse::Forbidden().finish()); }

    let limit = q.limit.unwrap_or(50).min(200) as i64;
    let rows = if let Some(before) = q.before {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT id, chat_id, sender_id, CASE WHEN is_deleted THEN '' ELSE content END as content, created_at
               FROM messages
               WHERE chat_id = $1 AND created_at < $2
               ORDER BY created_at DESC
               LIMIT $3"#,
        ).bind(chat_id).bind(before).bind(limit)
        .fetch_all(&state.pool).await.map_err(internal_err)?
    } else {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT id, chat_id, sender_id, CASE WHEN is_deleted THEN '' ELSE content END as content, created_at
               FROM messages
               WHERE chat_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
        ).bind(chat_id).bind(limit)
        .fetch_all(&state.pool).await.map_err(internal_err)?
    };

    Ok(HttpResponse::Ok().json(rows))
}

#[post("/api/chats/{chat_id}/leave")]
pub async fn leave_chat(state: web::Data<AppState>, path: web::Path<Uuid>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    sqlx::query("DELETE FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id).bind(user.0).execute(&mut *tx).await.map_err(internal_err)?;
    sqlx::query("DELETE FROM chat_members WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id).bind(user.0).execute(&mut *tx).await.map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

pub async fn ensure_direct_chat(pool: &Pool<Postgres>, a: Uuid, b: Uuid) -> anyhow::Result<Uuid> {
    #[derive(sqlx::FromRow)]
    struct ChatIdRow { id: Uuid }
    if let Some(row) = sqlx::query_as::<_, ChatIdRow>(
        r#"SELECT c.id
           FROM chats c
           JOIN chat_participants p1 ON p1.chat_id = c.id AND p1.user_id = $1
           JOIN chat_participants p2 ON p2.chat_id = c.id AND p2.user_id = $2
           WHERE c.is_direct = TRUE
           LIMIT 1"#,
    ).bind(a).bind(b).fetch_optional(pool).await? { return Ok(row.id); }

    let chat_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO chats (id, is_direct) VALUES ($1, TRUE)").bind(chat_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO chat_participants (chat_id, user_id) VALUES ($1, $2), ($1, $3)").bind(chat_id).bind(a).bind(b).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING").bind(chat_id).bind(a).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO chat_members (chat_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING").bind(chat_id).bind(b).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(chat_id)
}
