//! Validation and owned copies of user-selected images.
use std::path::Path;

pub fn image_extension(bytes: &[u8]) -> Result<&'static str, String> {
    if bytes.len() > 5 * 1024 * 1024 {
        return Err("封面图片不能超过 5 MB".into());
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Ok("jpg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Ok("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Ok("gif")
    } else {
        Err("请选择 PNG、JPEG、WebP 或 GIF 图片".into())
    }
}

pub fn copy_mystery_cover(source: &Path, destination_root: &Path) -> Result<String, String> {
    if std::fs::metadata(source).map_err(|e| e.to_string())?.len() > 5 * 1024 * 1024 {
        return Err("封面图片不能超过 5 MB".into());
    }
    let data = std::fs::read(source).map_err(|e| e.to_string())?;
    let extension = image_extension(&data)?;
    let destination =
        destination_root.join(format!("mystery-{:x}.{extension}", md5::compute(&data)));
    crate::platforms::storage::atomic_write(&destination, &data).map_err(|e| e.to_string())?;
    Ok(destination.display().to_string())
}
