use crate::auth::{internal_err, AuthUser};
use crate::state::AppState;
use crate::upload::CompressOpts;
use actix_multipart::Multipart;
use actix_web::{post, web, HttpResponse};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use sqlx::types::Uuid;

#[derive(sqlx::FromRow, serde::Serialize)]
pub struct FileDto {
    pub id: Uuid,
    pub content_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[post("/v1/api/files")] // generic files upload
pub async fn upload_files(
    state: web::Data<AppState>,
    mut payload: Multipart,
    _user: AuthUser,
    q: web::Query<std::collections::HashMap<String, String>>,
) -> actix_web::Result<HttpResponse> {
    let mut saved: Vec<FileDto> = Vec::new();
    let compress = CompressOpts {
        enabled: q.get("compress").map(|v| v == "true").unwrap_or(false),
        quality: q.get("quality").and_then(|v| v.parse().ok()).unwrap_or(80),
    };
    while let Some(Ok(mut field)) = payload.next().await {
        // read into memory and optionally recompress if image
        let ct = field
            .content_type()
            .map(|ct| ct.essence_str().to_string())
            .unwrap_or_else(|| "application/octet-stream".into());
        let mut buf = Vec::new();
        while let Some(chunk) = field.next().await {
            let c = chunk.map_err(internal_err)?;
            buf.extend_from_slice(&c);
        }

        let (bytes, content_type) = if compress.enabled && ct.starts_with("image/") {
            let img = image::load_from_memory(&buf).map_err(internal_err)?;
            let mut out = Vec::new();
            {
                let mut encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, compress.quality);
                let rgb = img.to_rgb8();
                encoder
                    .encode(
                        &rgb,
                        rgb.width(),
                        rgb.height(),
                        image::ColorType::Rgb8.into(),
                    )
                    .map_err(internal_err)?;
            }
            (out, "image/jpeg".to_string())
        } else {
            (buf, ct)
        };

        // compute sha256
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256 = format!("{:x}", hasher.finalize());

        // check if exists
        #[derive(sqlx::FromRow)]
        struct Exists {
            id: Uuid,
            content_type: String,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        if let Some(ex) = sqlx::query_as::<_, Exists>(
            "SELECT id, content_type, created_at FROM storage_files WHERE sha256 = $1",
        )
        .bind(&sha256)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?
        {
            saved.push(FileDto {
                id: ex.id,
                content_type: ex.content_type,
                created_at: ex.created_at,
            });
            continue;
        }

        let id = Uuid::new_v4();
        let mut out_path = state.storage_dir.as_ref().clone();
        out_path.push(&sha256);
        tokio::fs::write(&out_path, &bytes)
            .await
            .map_err(internal_err)?;
        let size = bytes.len() as i64;
        let rec = sqlx::query_as::<_, FileDto>(
            "INSERT INTO storage_files (id, path, content_type, sha256, size) VALUES ($1,$2,$3,$4,$5) RETURNING id, content_type, created_at"
        ).bind(id).bind(out_path.to_string_lossy().to_string()).bind(content_type).bind(&sha256).bind(size)
        .fetch_one(&state.pool).await.map_err(internal_err)?;
        saved.push(rec);
    }
    Ok(HttpResponse::Ok().json(saved))
}
