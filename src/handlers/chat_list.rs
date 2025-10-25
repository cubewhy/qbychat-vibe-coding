use crate::auth::{internal_err, AuthUser};
use crate::state::AppState;
use actix_web::{get, web, HttpResponse};
use serde::Deserialize;
use sqlx::types::Uuid;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct ListChatsQuery {
    pub include_unread: Option<bool>,
    pub include_first: Option<bool>,
}

#[get("/api/chats")]
pub async fn list_chats(
    state: web::Data<AppState>,
    user: AuthUser,
    q: web::Query<ListChatsQuery>,
) -> actix_web::Result<HttpResponse> {
    #[derive(sqlx::FromRow, serde::Serialize)]
    struct Row {
        id: Uuid,
        is_direct: bool,
        chat_type: Option<String>,
        title: Option<String>,
        created_at: chrono::DateTime<chrono::Utc>,
        pinned_message_id: Option<Uuid>,
        is_public: bool,
        public_handle: Option<String>,
    }
    let rows: Vec<Row> = sqlx::query_as::<_, Row>(
        "SELECT c.id, c.is_direct, c.chat_type, c.title, c.created_at, c.pinned_message_id, c.is_public, c.public_handle FROM chats c JOIN chat_participants p ON p.chat_id = c.id WHERE p.user_id = $1 ORDER BY c.created_at DESC"
    ).bind(user.0).fetch_all(&state.pool).await.map_err(internal_err)?;

    let pinned_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.pinned_message_id).collect();
    let pinned_map = load_pinned_messages(&state.pool, &pinned_ids).await?;

    #[derive(serde::Serialize)]
    struct ChatOut {
        id: Uuid,
        is_direct: bool,
        chat_type: Option<String>,
        title: Option<String>,
        created_at: chrono::DateTime<chrono::Utc>,
        unread: Option<i64>,
        first_message: Option<serde_json::Value>,
        is_public: bool,
        public_handle: Option<String>,
        pinned_message: Option<serde_json::Value>,
    }
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let mut unread: Option<i64> = None;
        let mut first: Option<serde_json::Value> = None;
        if q.include_unread.unwrap_or(false) {
            let lr: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
                "SELECT m.created_at FROM chat_members cm LEFT JOIN messages m ON m.id = cm.last_read_message_id WHERE cm.chat_id = $1 AND cm.user_id = $2"
            ).bind(r.id).bind(user.0).fetch_optional(&state.pool).await.map_err(internal_err)?;
            let c: i64 = if let Some(t) = lr {
                sqlx::query_scalar(
                "SELECT COUNT(*) FROM messages WHERE chat_id = $1 AND is_deleted = FALSE AND created_at > $2"
            ).bind(r.id).bind(t).fetch_one(&state.pool).await.map_err(internal_err)?
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM messages WHERE chat_id = $1 AND is_deleted = FALSE",
                )
                .bind(r.id)
                .fetch_one(&state.pool)
                .await
                .map_err(internal_err)?
            };
            unread = Some(c);
        }
        if q.include_first.unwrap_or(false) {
            #[derive(sqlx::FromRow)]
            struct M {
                id: Uuid,
                content: String,
                created_at: chrono::DateTime<chrono::Utc>,
            }
            if let Some(m) = sqlx::query_as::<_, M>(
                "SELECT id, content, created_at FROM messages WHERE chat_id = $1 AND is_deleted = FALSE ORDER BY created_at ASC LIMIT 1"
            ).bind(r.id).fetch_optional(&state.pool).await.map_err(internal_err)? {
                first = Some(serde_json::json!({"id": m.id, "content": m.content, "created_at": m.created_at}));
            }
        }
        let pinned_msg = r
            .pinned_message_id
            .and_then(|pid| pinned_map.get(&pid).cloned());
        out.push(ChatOut {
            id: r.id,
            is_direct: r.is_direct,
            chat_type: r.chat_type,
            title: r.title,
            created_at: r.created_at,
            unread,
            first_message: first,
            is_public: r.is_public,
            public_handle: r.public_handle,
            pinned_message: pinned_msg,
        });
    }
    Ok(HttpResponse::Ok().json(out))
}

async fn load_pinned_messages(
    pool: &sqlx::PgPool,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, serde_json::Value>, actix_web::Error> {
    use std::collections::HashMap;
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        chat_id: Uuid,
        content: String,
        sender_id: Uuid,
        username: String,
        created_at: chrono::DateTime<chrono::Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT m.id, m.chat_id, CASE WHEN m.is_deleted THEN '' ELSE m.content END as content, m.sender_id, u.username, m.created_at FROM messages m JOIN users u ON u.id = m.sender_id WHERE m.id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(internal_err)?;
    let mut map = HashMap::new();
    for row in rows {
        map.insert(
            row.id,
            serde_json::json!({
                "id": row.id,
                "chat_id": row.chat_id,
                "content": row.content,
                "sender": {"id": row.sender_id, "username": row.username},
                "created_at": row.created_at,
            }),
        );
    }
    Ok(map)
}
