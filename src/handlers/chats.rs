use actix_web::{get, post, web, HttpResponse};
use sqlx::{Pool, Postgres};
use sqlx::types::Uuid;
use crate::state::AppState;
use crate::auth::{AuthUser, internal_err};
use crate::models::{CreateDirectChatReq, ListQuery, MessageRow};

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
            r#"SELECT id, chat_id, sender_id, content, created_at
               FROM messages
               WHERE chat_id = $1 AND created_at < $2
               ORDER BY created_at DESC
               LIMIT $3"#,
        ).bind(chat_id).bind(before).bind(limit)
        .fetch_all(&state.pool).await.map_err(internal_err)?
    } else {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT id, chat_id, sender_id, content, created_at
               FROM messages
               WHERE chat_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
        ).bind(chat_id).bind(limit)
        .fetch_all(&state.pool).await.map_err(internal_err)?
    };

    Ok(HttpResponse::Ok().json(rows))
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
    tx.commit().await?;
    Ok(chat_id)
}
