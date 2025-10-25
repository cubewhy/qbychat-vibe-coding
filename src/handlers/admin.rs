use crate::auth::internal_err;
use crate::state::AppState;
use actix_web::{post, web, HttpResponse};
use tracing::info;

#[post("/api/admin/storage/purge")]
pub async fn purge_storage(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
) -> actix_web::Result<HttpResponse> {
    let admin_token = std::env::var("ADMIN_TOKEN").unwrap_or_default();
    let header = req
        .headers()
        .get("X-Admin-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if admin_token.is_empty() || header != admin_token {
        return Ok(HttpResponse::Unauthorized().finish());
    }
    let deleted = purge_unreferenced_internal(&state)
        .await
        .map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"deleted": deleted})))
}

pub async fn purge_unreferenced_internal(state: &AppState) -> anyhow::Result<usize> {
    // find files not referenced by message_attachments
    #[derive(sqlx::FromRow)]
    struct Row {
        id: sqlx::types::Uuid,
        path: String,
    }
    let rows: Vec<Row> = sqlx::query_as::<_, Row>(
        "SELECT f.id, f.path FROM storage_files f LEFT JOIN message_attachments m ON m.file_id = f.id WHERE m.file_id IS NULL"
    ).fetch_all(&state.pool).await?;

    let mut deleted = 0usize;
    let mut tx = state.pool.begin().await?;
    for r in rows {
        // delete db row first to avoid races
        sqlx::query("DELETE FROM storage_files WHERE id = $1")
            .bind(r.id)
            .execute(&mut *tx)
            .await?;
        // best-effort remove file
        if tokio::fs::remove_file(&r.path).await.is_ok() {
            info!(path = %r.path, "purged file");
        }
        deleted += 1;
    }
    tx.commit().await?;
    Ok(deleted)
}
