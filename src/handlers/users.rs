use crate::auth::{internal_err, AuthUser};
use crate::state::AppState;
use crate::upload::{save_part, CompressOpts};
use actix_multipart::Multipart;
use actix_web::{get, post, web, HttpResponse};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

#[derive(Serialize, sqlx::FromRow)]
pub struct AvatarDto {
    pub id: Uuid,
    pub content_type: String,
    pub is_primary: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[post("/api/users/me/avatars")]
pub async fn upload_avatars(
    state: web::Data<AppState>,
    mut payload: Multipart,
    user: AuthUser,
    q: web::Query<std::collections::HashMap<String, String>>,
) -> actix_web::Result<HttpResponse> {
    let mut saved: Vec<AvatarDto> = Vec::new();
    let compress = CompressOpts {
        enabled: q.get("compress").map(|v| v == "true").unwrap_or(false),
        quality: q.get("quality").and_then(|v| v.parse().ok()).unwrap_or(80),
    };
    while let Some(Ok(field)) = payload.next().await {
        let id = Uuid::new_v4();
        let (path, ct) = save_part(field, &state.storage_dir, &id.to_string(), &compress)
            .await
            .map_err(internal_err)?;
        let rec = sqlx::query_as::<_, AvatarDto>(
            "INSERT INTO user_avatars (id, user_id, path, content_type) VALUES ($1,$2,$3,$4) RETURNING id, content_type, is_primary, created_at"
        )
        .bind(id).bind(user.0).bind(path.to_string_lossy().to_string()).bind(ct)
        .fetch_one(&state.pool).await.map_err(internal_err)?;
        saved.push(rec);
    }
    Ok(HttpResponse::Ok().json(saved))
}

#[derive(Deserialize)]
pub struct SetPrimaryReq {
    pub avatar_id: Uuid,
}

#[post("/api/users/me/avatars/primary")]
pub async fn set_primary(
    state: web::Data<AppState>,
    user: AuthUser,
    req: web::Json<SetPrimaryReq>,
) -> actix_web::Result<HttpResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_err)?;
    let owner =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT user_id FROM user_avatars WHERE id = $1")
            .bind(req.avatar_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(internal_err)?;
    if owner != Some(user.0) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    sqlx::query("UPDATE user_avatars SET is_primary = FALSE WHERE user_id = $1")
        .bind(user.0)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
    sqlx::query("UPDATE user_avatars SET is_primary = TRUE WHERE id = $1")
        .bind(req.avatar_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
    tx.commit().await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().finish())
}

#[get("/api/users/{user_id}/avatars")]
pub async fn list_avatars(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> actix_web::Result<HttpResponse> {
    let user_id = path.into_inner();
    let rows = sqlx::query_as::<_, AvatarDto>(
        "SELECT id, content_type, is_primary, created_at FROM user_avatars WHERE user_id = $1 ORDER BY created_at DESC"
    ).bind(user_id).fetch_all(&state.pool).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(rows))
}
