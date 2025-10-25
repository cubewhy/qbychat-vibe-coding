use actix_web::{post, web, HttpResponse};
use serde::Deserialize;
use sqlx::types::Uuid;
use crate::state::AppState;
use crate::auth::{AuthUser, internal_err};

#[derive(Deserialize)]
pub struct SendMessageReq { pub content: String }

#[post("/api/chats/{chat_id}/messages")]
pub async fn send_message(state: web::Data<AppState>, path: web::Path<Uuid>, user: AuthUser, req: web::Json<SendMessageReq>) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let is_member = sqlx::query_scalar::<_, Option<i64>>("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id).bind(user.0).fetch_one(&state.pool).await.map_err(internal_err)?.is_some();
    if !is_member { return Ok(HttpResponse::Forbidden().finish()); }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO messages (id, chat_id, sender_id, content) VALUES ($1,$2,$3,$4)")
        .bind(id).bind(chat_id).bind(user.0).bind(req.content.trim())
        .execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"id": id})))
}

#[derive(Deserialize)]
pub struct EditMessageReq { pub content: String }

#[post("/api/messages/{message_id}/edit")]
pub async fn edit_message(state: web::Data<AppState>, path: web::Path<Uuid>, user: AuthUser, req: web::Json<EditMessageReq>) -> actix_web::Result<HttpResponse> {
    let message_id = path.into_inner();
    let res = sqlx::query("UPDATE messages SET content = $1, edited_at = now() WHERE id = $2 AND sender_id = $3 AND is_deleted = FALSE")
        .bind(req.content.trim()).bind(message_id).bind(user.0)
        .execute(&state.pool).await.map_err(internal_err)?;
    if res.rows_affected() == 0 { return Ok(HttpResponse::Forbidden().finish()); }
    Ok(HttpResponse::Ok().finish())
}

#[post("/api/messages/{message_id}/delete")]
pub async fn delete_message(state: web::Data<AppState>, path: web::Path<Uuid>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    let message_id = path.into_inner();
    let res = sqlx::query("UPDATE messages SET is_deleted = TRUE, deleted_at = now() WHERE id = $1 AND sender_id = $2")
        .bind(message_id).bind(user.0)
        .execute(&state.pool).await.map_err(internal_err)?;
    if res.rows_affected() == 0 { return Ok(HttpResponse::Forbidden().finish()); }
    Ok(HttpResponse::Ok().finish())
}

#[derive(Deserialize)]
pub struct ReadBulkReq { pub chat_id: Uuid, pub message_ids: Vec<Uuid> }

#[post("/api/messages/read_bulk")]
pub async fn read_bulk(state: web::Data<AppState>, user: AuthUser, req: web::Json<ReadBulkReq>) -> actix_web::Result<HttpResponse> {
    let chat_id = req.chat_id;
    // validate membership
    let is_member = sqlx::query_scalar::<_, Option<i64>>("SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id).bind(user.0).fetch_one(&state.pool).await.map_err(internal_err)?.is_some();
    if !is_member { return Ok(HttpResponse::Forbidden().finish()); }

    #[derive(sqlx::FromRow)]
    struct Meta { is_direct: bool, chat_type: Option<String> }
    let meta = sqlx::query_as::<_, Meta>("SELECT is_direct, chat_type FROM chats WHERE id = $1")
        .bind(chat_id).fetch_one(&state.pool).await.map_err(internal_err)?;

    // participant count
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_participants WHERE chat_id = $1").bind(chat_id).fetch_one(&state.pool).await.map_err(internal_err)?;

    let now = chrono::Utc::now();

    if meta.chat_type.as_deref() == Some("channel") {
        // increment views for each message
        for mid in &req.message_ids {
            sqlx::query("INSERT INTO message_views (message_id, views, last_view_at) VALUES ($1, 1, $2) ON CONFLICT (message_id) DO UPDATE SET views = message_views.views + 1, last_view_at = EXCLUDED.last_view_at")
                .bind(mid).bind(now).execute(&state.pool).await.map_err(internal_err)?;
        }
    } else if meta.is_direct || count > 100 {
        for mid in &req.message_ids {
            sqlx::query("INSERT INTO message_reads_agg (message_id, is_read, first_read_at) VALUES ($1, TRUE, $2) ON CONFLICT (message_id) DO UPDATE SET is_read = TRUE, first_read_at = COALESCE(message_reads_agg.first_read_at, EXCLUDED.first_read_at)")
                .bind(mid).bind(now).execute(&state.pool).await.map_err(internal_err)?;
        }
    } else {
        for mid in &req.message_ids {
            sqlx::query("INSERT INTO message_reads_small (message_id, user_id, read_at) VALUES ($1,$2,$3) ON CONFLICT (message_id, user_id) DO UPDATE SET read_at = EXCLUDED.read_at")
                .bind(mid).bind(user.0).bind(now).execute(&state.pool).await.map_err(internal_err)?;
        }
    }
    Ok(HttpResponse::Ok().finish())
}

#[post("/api/admin/reads/purge")]
pub async fn purge_reads(state: web::Data<AppState>) -> actix_web::Result<HttpResponse> {
    let threshold = chrono::Utc::now() - chrono::Duration::days(7);
    let res = sqlx::query("DELETE FROM message_reads_small WHERE read_at < $1")
        .bind(threshold).execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"deleted": res.rows_affected()})))
}
