use actix_multipart::Field;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};

pub struct CompressOpts {
    pub enabled: bool,
    pub quality: u8,
}

pub async fn save_part(
    mut field: Field,
    base: &Path,
    id: &str,
    compress: &CompressOpts,
) -> anyhow::Result<(PathBuf, String)> {
    let ct = field
        .content_type()
        .map(|ct| ct.essence_str().to_string())
        .unwrap_or_else(|| "application/octet-stream".into());
    let mut out_path = base.to_path_buf();
    out_path.push(id);

    if compress.enabled && ct.starts_with("image/") {
        // read into memory then re-encode as JPEG (drops EXIF)
        let mut buf = Vec::new();
        while let Some(chunk) = field.next().await {
            let c = chunk.map_err(|e| anyhow::anyhow!(e.to_string()))?;
            buf.extend_from_slice(&c);
        }
        let img = image::load_from_memory(&buf)?;
        let mut out = Vec::new();
        {
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, compress.quality);
            let rgb = img.to_rgb8();
            encoder.encode(
                &rgb,
                rgb.width(),
                rgb.height(),
                image::ColorType::Rgb8.into(),
            )?;
        }
        tokio::fs::write(&out_path, &out).await?;
        Ok((out_path, "image/jpeg".into()))
    } else {
        let mut f = tokio::fs::File::create(&out_path).await?;
        while let Some(chunk) = field.next().await {
            let data = chunk.map_err(|e| anyhow::anyhow!(e.to_string()))?;
            tokio::io::copy(&mut &data[..], &mut f).await?;
        }
        Ok((out_path, ct))
    }
}
