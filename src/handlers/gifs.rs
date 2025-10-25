use actix_web::{get, post, web, HttpResponse};
use serde::Deserialize;
use sqlx::types::Uuid;
use tracing::instrument;

use crate::auth::{internal_err, AuthUser};
use crate::models::GifSendReq;
use crate::state::AppState;

use super::messages::ensure_can_send;

#[derive(Deserialize)]
pub struct GifSearchQuery {
    pub q: String,
    pub limit: Option<u8>,
}

#[get("/api/gifs/search")]
pub async fn search_gifs(
    state: web::Data<AppState>,
    _user: AuthUser,
    q: web::Query<GifSearchQuery>,
) -> actix_web::Result<HttpResponse> {
    let provider = state
        .gif_provider
        .as_ref()
        .ok_or_else(|| actix_web::error::ErrorServiceUnavailable("gif provider disabled"))?;
    let limit = q.limit.unwrap_or(20).clamp(1, 50);
    let results = provider
        .search(q.q.trim(), limit)
        .await
        .map_err(|e| actix_web::error::ErrorBadGateway(e.to_string()))?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"results": results})))
}

#[post("/api/chats/{chat_id}/gifs")]
#[instrument(skip(state, req, user))]
pub async fn send_gif(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: web::Json<GifSendReq>,
) -> actix_web::Result<HttpResponse> {
    let provider = state
        .gif_provider
        .as_ref()
        .ok_or_else(|| actix_web::error::ErrorServiceUnavailable("gif provider disabled"))?;
    let chat_id = path.into_inner();
    ensure_can_send(&state, chat_id, user.0).await?;

    if provider.provider().to_lowercase() != req.provider.to_lowercase() {
        return Ok(HttpResponse::BadRequest().body("unknown provider"));
    }

    if req.gif_url.trim().is_empty() || req.gif_preview_url.trim().is_empty() {
        return Ok(HttpResponse::BadRequest().body("gif_url required"));
    }

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO messages (id, chat_id, sender_id, content, kind, gif_id, gif_url, gif_preview_url, gif_provider) VALUES ($1,$2,$3,'', 'gif',$4,$5,$6,$7)",
    )
    .bind(id)
    .bind(chat_id)
    .bind(user.0)
    .bind(&req.gif_id)
    .bind(&req.gif_url)
    .bind(&req.gif_preview_url)
    .bind(req.provider.to_lowercase())
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({"id": id, "kind": "gif"})))
}
