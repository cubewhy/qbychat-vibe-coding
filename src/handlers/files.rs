use crate::auth::{internal_err, AuthUser};
use crate::state::AppState;
use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

#[derive(Deserialize)]
pub struct DownloadTokenReq {
    pub avatar_id: Uuid,
}

#[derive(Serialize)]
pub struct DownloadTokenResp {
    pub token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[post("/v1/api/files/download_token")]
pub async fn create_download_token(
    state: web::Data<AppState>,
    _user: AuthUser,
    req: web::Json<DownloadTokenReq>,
) -> actix_web::Result<HttpResponse> {
    // Ensure avatar exists
    #[derive(sqlx::FromRow)]
    struct Row {
        path: String,
        content_type: String,
    }
    let row = sqlx::query_as::<_, Row>("SELECT path, content_type FROM user_avatars WHERE id = $1")
        .bind(req.avatar_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("avatar not found"))?;
    let token = Uuid::new_v4().to_string();
    let ttl = state.download_token_ttl;
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(ttl as i64);
    if let Some(client) = &state.redis {
        let mut conn = client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(internal_err)?;
        let key = format!("dl:{}", token);
        let val = format!("{}|{}", row.path, row.content_type);
        redis::Cmd::set_ex(key, val, ttl)
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(internal_err)?;
    } else {
        return Ok(HttpResponse::ServiceUnavailable().body("no redis"));
    }
    Ok(HttpResponse::Ok().json(DownloadTokenResp { token, expires_at }))
}

#[get("/v1/api/files/{token}")]
pub async fn download_file(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> actix_web::Result<HttpResponse> {
    let token = path.into_inner();
    let Some(client) = &state.redis else {
        return Ok(HttpResponse::ServiceUnavailable().finish());
    };
    let mut conn = client
        .get_multiplexed_tokio_connection()
        .await
        .map_err(internal_err)?;
    let key = format!("dl:{}", token);
    let v: Option<String> = redis::Cmd::get(key)
        .query_async(&mut conn)
        .await
        .map_err(internal_err)?;
    let Some(v) = v else {
        return Ok(HttpResponse::NotFound().finish());
    };
    let mut it = v.splitn(2, '|');
    let p = it.next().unwrap_or_default().to_string();
    let ct = it.next().unwrap_or("application/octet-stream");
    let bytes = tokio::fs::read(&p).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().content_type(ct).body(bytes))
}
