use actix_web::{delete, get, post, web, HttpResponse};
use sqlx::types::Uuid;
use tracing::instrument;

use crate::auth::{internal_err, AuthUser};
use crate::models::{SendStickerReq, StickerCreateReq, StickerPackCreateReq};
use crate::state::AppState;

use super::messages::ensure_can_send;

#[post("/api/sticker_packs")]
#[instrument(skip(state, req, user))]
pub async fn create_pack(
    state: web::Data<AppState>,
    user: AuthUser,
    req: web::Json<StickerPackCreateReq>,
) -> actix_web::Result<HttpResponse> {
    let short = req.short_name.trim().to_lowercase();
    if short.is_empty()
        || !short
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Ok(HttpResponse::BadRequest().body("invalid short_name"));
    }
    let exists =
        sqlx::query_scalar::<_, Option<i32>>("SELECT 1 FROM sticker_packs WHERE short_name = $1")
            .bind(&short)
            .fetch_one(&state.pool)
            .await
            .map_err(internal_err)?
            .is_some();
    if exists {
        return Ok(HttpResponse::Conflict().body("short_name taken"));
    }
    let pack_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sticker_packs (id, title, short_name, created_by) VALUES ($1,$2,$3,$4)",
    )
    .bind(pack_id)
    .bind(req.title.trim())
    .bind(&short)
    .bind(user.0)
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;
    Ok(HttpResponse::Ok()
        .json(serde_json::json!({"id": pack_id, "title": req.title, "short_name": short})))
}

#[post("/api/sticker_packs/{pack_id}/stickers")]
#[instrument(skip(state, req, user))]
pub async fn add_sticker(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: web::Json<StickerCreateReq>,
) -> actix_web::Result<HttpResponse> {
    let pack_id = path.into_inner();
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT created_by FROM sticker_packs WHERE id = $1")
            .bind(pack_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal_err)?;
    let Some(owner_id) = owner else {
        return Ok(HttpResponse::NotFound().finish());
    };
    if owner_id != user.0 {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let exists = sqlx::query_scalar::<_, Option<i32>>("SELECT 1 FROM storage_files WHERE id = $1")
        .bind(req.file_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?
        .is_some();
    if !exists {
        return Ok(HttpResponse::BadRequest().body("file not found"));
    }
    let sticker_id = Uuid::new_v4();
    sqlx::query("INSERT INTO stickers (id, pack_id, emoji, file_id) VALUES ($1,$2,$3,$4)")
        .bind(sticker_id)
        .bind(pack_id)
        .bind(req.emoji.as_deref())
        .bind(req.file_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok()
        .json(serde_json::json!({"id": sticker_id, "pack_id": pack_id, "emoji": req.emoji})))
}

#[post("/api/sticker_packs/{pack_id}/install")]
pub async fn install_pack(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let pack_id = path.into_inner();
    let exists = sqlx::query_scalar::<_, Option<i32>>("SELECT 1 FROM sticker_packs WHERE id = $1")
        .bind(pack_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?
        .is_some();
    if !exists {
        return Ok(HttpResponse::NotFound().finish());
    }
    sqlx::query(
        "INSERT INTO user_sticker_packs (user_id, pack_id) VALUES ($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(user.0)
    .bind(pack_id)
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[delete("/api/sticker_packs/{pack_id}/install")]
pub async fn uninstall_pack(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let pack_id = path.into_inner();
    sqlx::query("DELETE FROM user_sticker_packs WHERE user_id = $1 AND pack_id = $2")
        .bind(user.0)
        .bind(pack_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[get("/api/me/sticker_packs")]
pub async fn list_my_packs(
    state: web::Data<AppState>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    #[derive(sqlx::FromRow, serde::Serialize)]
    struct Row {
        pack_id: Uuid,
        title: String,
        short_name: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT sp.id as pack_id, sp.title, sp.short_name FROM user_sticker_packs usp JOIN sticker_packs sp ON sp.id = usp.pack_id WHERE usp.user_id = $1 ORDER BY usp.installed_at DESC",
    )
    .bind(user.0)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(rows))
}

#[post("/api/chats/{chat_id}/stickers")]
#[instrument(skip(state, req, user))]
pub async fn send_sticker(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: web::Json<SendStickerReq>,
) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    ensure_can_send(&state, chat_id, user.0).await?;

    #[derive(sqlx::FromRow)]
    struct StickerMeta {
        pack_id: Uuid,
        created_by: Uuid,
    }
    let sticker = sqlx::query_as::<_, StickerMeta>(
        "SELECT s.pack_id, sp.created_by FROM stickers s JOIN sticker_packs sp ON sp.id = s.pack_id WHERE s.id = $1",
    )
    .bind(req.sticker_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?
    .ok_or_else(|| actix_web::error::ErrorNotFound("sticker not found"))?;

    if sticker.created_by != user.0 {
        let installed = sqlx::query_scalar::<_, Option<i32>>(
            "SELECT 1 FROM user_sticker_packs WHERE user_id = $1 AND pack_id = $2",
        )
        .bind(user.0)
        .bind(sticker.pack_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?
        .is_some();
        if !installed {
            return Ok(HttpResponse::Forbidden().body("install pack first"));
        }
    }

    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO messages (id, chat_id, sender_id, content, kind, sticker_id) VALUES ($1,$2,$3,'', 'sticker',$4)")
        .bind(id)
        .bind(chat_id)
        .bind(user.0)
        .bind(req.sticker_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({"id": id, "kind": "sticker"})))
}
