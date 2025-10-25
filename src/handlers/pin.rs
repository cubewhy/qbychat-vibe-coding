use crate::auth::{internal_err, AuthUser};
use crate::state::AppState;
use actix_web::{post, web, HttpResponse};
use sqlx::types::Uuid;

#[post("/api/chats/{chat_id}/pin_message")]
pub async fn pin_message(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<serde_json::Value>,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    let message_id: Uuid = serde_json::from_value(
        body.get("message_id")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    )
    .map_err(|_| actix_web::error::ErrorBadRequest("message_id"))?;
    #[derive(sqlx::FromRow)]
    struct Meta {
        owner_id: Option<Uuid>,
    }
    let meta = sqlx::query_as::<_, Meta>("SELECT owner_id FROM chats WHERE id = $1")
        .bind(chat_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))?;
    let can_pin: Option<bool> = sqlx::query_scalar(
        "SELECT can_pin_messages FROM chat_admin_permissions WHERE chat_id=$1 AND user_id=$2",
    )
    .bind(chat_id)
    .bind(user.0)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?;
    if meta.owner_id != Some(user.0) && !can_pin.unwrap_or(false) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    // message must be in chat
    let ok =
        sqlx::query_scalar::<_, Option<i64>>("SELECT 1 FROM messages WHERE id=$1 AND chat_id=$2")
            .bind(message_id)
            .bind(chat_id)
            .fetch_one(&state.pool)
            .await
            .map_err(internal_err)?
            .is_some();
    if !ok {
        return Ok(HttpResponse::BadRequest().body("invalid message_id"));
    }
    sqlx::query("UPDATE chats SET pinned_message_id = $1 WHERE id = $2")
        .bind(message_id)
        .bind(chat_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/api/chats/{chat_id}/unpin_message")]
pub async fn unpin_message(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    #[derive(sqlx::FromRow)]
    struct Meta {
        owner_id: Option<Uuid>,
    }
    let meta = sqlx::query_as::<_, Meta>("SELECT owner_id FROM chats WHERE id = $1")
        .bind(chat_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("chat not found"))?;
    let can_pin: Option<bool> = sqlx::query_scalar(
        "SELECT can_pin_messages FROM chat_admin_permissions WHERE chat_id=$1 AND user_id=$2",
    )
    .bind(chat_id)
    .bind(user.0)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?;
    if meta.owner_id != Some(user.0) && !can_pin.unwrap_or(false) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    sqlx::query("UPDATE chats SET pinned_message_id = NULL WHERE id = $1")
        .bind(chat_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}
