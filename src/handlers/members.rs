use crate::auth::{internal_err, AuthUser};
use crate::state::AppState;
use actix_web::{delete, get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

#[derive(Deserialize)]
pub struct NoteReq {
    pub note: String,
}

#[derive(Serialize)]
pub struct NoteResp {
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct NotifyReq {
    pub mute_forever: Option<bool>,
    pub mute_until: Option<chrono::DateTime<chrono::Utc>>,
    pub notify_type: Option<String>,
}

#[derive(Serialize)]
pub struct NotifyResp {
    pub mute_forever: bool,
    pub mute_until: Option<chrono::DateTime<chrono::Utc>>,
    pub notify_type: String,
}

fn forbidden() -> actix_web::Result<HttpResponse> {
    Ok(HttpResponse::Forbidden().finish())
}

async fn ensure_member(
    state: &AppState,
    chat_id: Uuid,
    user_id: Uuid,
) -> Result<bool, actix_web::Error> {
    let is_member = sqlx::query_scalar::<_, Option<i32>>(
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

#[get("/v1/api/chats/{chat_id}/member/note")]
pub async fn get_note(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state, chat_id, user.0).await? {
        return forbidden();
    }
    let note: Option<String> =
        sqlx::query_scalar("SELECT note FROM chat_members WHERE chat_id = $1 AND user_id = $2")
            .bind(chat_id)
            .bind(user.0)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(NoteResp { note }))
}

#[post("/v1/api/chats/{chat_id}/member/note")]
pub async fn set_note(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: web::Json<NoteReq>,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state, chat_id, user.0).await? {
        return forbidden();
    }
    sqlx::query("INSERT INTO chat_members (chat_id, user_id, note) VALUES ($1,$2,$3) ON CONFLICT (chat_id,user_id) DO UPDATE SET note = EXCLUDED.note")
        .bind(chat_id).bind(user.0).bind(req.note.trim()).execute(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[delete("/v1/api/chats/{chat_id}/member/note")]
pub async fn delete_note(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    if !ensure_member(&state, chat_id, user.0).await? {
        return forbidden();
    }
    sqlx::query("UPDATE chat_members SET note = NULL WHERE chat_id = $1 AND user_id = $2")
        .bind(chat_id)
        .bind(user.0)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}
