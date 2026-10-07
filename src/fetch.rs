use anyhow::Result;
use std::path::{Path, PathBuf};

pub async fn fetch_url_to_file(url: &str, dest_dir: &Path) -> Result<PathBuf> {
    let url = url.trim().trim_matches('"').trim_matches('\'').to_string();
    if !url.starts_with("http") {
        anyhow::bail!("URL must start with http/https");
    }
    let _ = crate::ytdlp::ensure_ytdlp().await;
    std::fs::create_dir_all(dest_dir)?;
    let resolved = crate::extractor::resolve_url(&url).await;
    let info_url = if resolved != url { resolved } else { url };
    let info = crate::extractor::extract_info(&info_url).await?;
    let fallback = crate::util::extract_id_from_url(&info_url);
    if info.is_video || info.images.is_empty() {
        let title = if info.title.is_empty() { fallback.unwrap_or_else(|| "download".to_string()) } else { info.title };
        let out = crate::util::output_path_from_title(&title, None, &Some(dest_dir.to_path_buf()), "mp4");
        crate::downloader::download_direct_video(&info_url, &out).await?;
        return Ok(out);
    }
    let tmp = tempfile::tempdir()?;
    let paths = crate::downloader::download_all_images(info.images.clone(), tmp.path()).await?;
    let audio = match info.music_url {
        Some(m) => crate::downloader::download_audio(&m, tmp.path()).await,
        None => None,
    };
    let out = crate::util::output_path_from_title(&info.title, fallback.as_deref(), &Some(dest_dir.to_path_buf()), "mp4");
    crate::video::create_swipe_video(paths, audio, &out, 30, 0.6, None, 1080, 1920, 0, 0)?;
    Ok(out)
}
