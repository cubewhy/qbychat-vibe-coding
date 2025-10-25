use super::members::{NotifyReq, NotifyResp};
use crate::auth::{internal_err, AuthUser};
use crate::state::AppState;
use actix_web::{delete, get, post, web, HttpResponse};
use sqlx::types::Uuid;

async fn ensure_member(
    state: &AppState,
    chat_id: Uuid,
    user_id: Uuid,
) -> Result<bool, actix_web::Error> {
    let is_member = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(internal_err)?
    .is_some();
    Ok(is_member)
}

#[get("/api/chats/{chat_id}/member/notify")]
pub async fn get_notify(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let r: Option<(bool, Option<chrono::DateTime<chrono::Utc>>, String)> = sqlx::query_as(
        "SELECT mute_forever, mute_until, notify_type FROM chat_members WHERE chat_id = $1 AND user_id = $2"
    ).bind(chat_id).bind(user.0).fetch_optional(&state.pool).await.map_err(internal_err)?;
    let (mute_forever, mute_until, notify_type) = r.unwrap_or((false, None, "all".to_string()));
    Ok(HttpResponse::Ok().json(NotifyResp {
        mute_forever,
        mute_until,
        notify_type,
    }))
}

#[post("/api/chats/{chat_id}/member/notify")]
pub async fn set_notify(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: web::Json<NotifyReq>,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let mute_forever = req.mute_forever.unwrap_or(false);
    let mute_until = req.mute_until;
    let notify_type = req.notify_type.clone().unwrap_or_else(|| "all".into());
    sqlx::query("INSERT INTO chat_members (chat_id, user_id, mute_forever, mute_until, notify_type) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (chat_id,user_id) DO UPDATE SET mute_forever = EXCLUDED.mute_forever, mute_until = EXCLUDED.mute_until, notify_type = EXCLUDED.notify_type")
        .bind(chat_id).bind(user.0).bind(mute_forever).bind(mute_until).bind(&notify_type).execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[get("/api/chats/{chat_id}/member/mentions")]
pub async fn get_mentions(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }
    #[derive(sqlx::FromRow, serde::Serialize)]
    struct Row {
        message_id: Uuid,
        chat_id: Uuid,
        excerpt: String,
        created_at: chrono::DateTime<chrono::Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT message_id, chat_id, excerpt, created_at FROM member_mentions WHERE chat_id = $1 AND user_id = $2 ORDER BY created_at DESC LIMIT 200"
    )
    .bind(chat_id)
    .bind(user.0)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"mentions": rows})))
}

#[delete("/api/chats/{chat_id}/member/mentions")]
pub async fn clear_mentions(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state, chat_id, user.0).await? {
        return Ok(HttpResponse::Forbidden().finish());
    }
    sqlx::query("DELETE FROM member_mentions WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(user.0)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}
