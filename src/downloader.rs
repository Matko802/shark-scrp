use anyhow::{bail, Result};
use futures_util::StreamExt;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use crate::ytdlp;

pub type ProgressCb = Arc<dyn Fn(u64, u64) + Send + Sync>;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

async fn download_via_ytdlp(url: &str, dest: &Path) -> Result<()> {
    let ytdlp_bin = ytdlp::ytdlp_path_async().await;

    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let tmp_name = format!("sharktmp_{}_{}.mp4", std::process::id(), rand::random::<u32>());
    let tmp_path = parent.join(&tmp_name);
    let tmp_str = tmp_path.to_string_lossy().to_string();
    let output = tokio::process::Command::new(&ytdlp_bin)
        .args(["--no-warnings", "--no-playlist", "-o", &tmp_str, url])
        .output()
        .await?;
    if !output.status.success() {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("yt-dlp failed: {}", stderr.lines().next().unwrap_or("unknown"));
    }
    let meta = tokio::fs::metadata(&tmp_path).await?;
    if meta.len() < 500 {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        bail!("File too small");
    }
    if let Some(parent_dir) = dest.parent() {
        if !parent_dir.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent_dir).await?;
        }
    }
    match tokio::fs::rename(&tmp_path, dest).await {
        Ok(_) => {}
        Err(_) => {
            tokio::fs::copy(&tmp_path, dest).await?;
            let _ = tokio::fs::remove_file(&tmp_path).await;
        }
    }
    Ok(())
}

pub async fn download_direct_video(url: &str, dest: &Path) -> Result<()> {

    download_via_ytdlp(url, dest).await
}

pub async fn download_file(url: &str, dest: &Path, _desc: &str) -> Result<()> {
    download_file_with_progress(url, dest, None).await
}

pub async fn download_file_with_progress(url: &str, dest: &Path, progress: Option<ProgressCb>) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let mut fixed_url = url.to_string();
    fixed_url = fixed_url.replace("\\u002F", "/");
    if fixed_url.starts_with("//") {
        fixed_url = format!("https:{}", fixed_url);
    } else if fixed_url.starts_with('/') {

        if fixed_url.starts_with("/video/") || fixed_url.starts_with("/music/") {
            fixed_url = format!("https://www.tikwm.com{}", fixed_url);
        } else {
            fixed_url = format!("https://www.tiktok.com{}", fixed_url);
        }
    }

    let referer = if fixed_url.contains("cdninstagram.com") || fixed_url.contains("instagram.com") {
        "https://www.instagram.com/"
    } else if fixed_url.contains("tikwm.com") {
        "https://www.tikwm.com/"
    } else if fixed_url.contains("tiktokcdn") || fixed_url.contains("muscdn") || fixed_url.contains("tiktok.com") {
        "https://www.tiktok.com/"
    } else {
        "https://www.tiktok.com/"
    };
    let resp = match client
        .get(&fixed_url)
        .header("User-Agent", UA)
        .header("Referer", referer)
        .send()
        .await {
            Ok(r) => r,
            Err(_) => {
                return download_via_ytdlp(&fixed_url, dest).await;
            }
        };
    if !resp.status().is_success() {
        return download_via_ytdlp(&fixed_url, dest).await;
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if let Some(cb) = &progress {
            cb(downloaded, total);
        }
    }
    file.flush().await?;
    let meta = tokio::fs::metadata(dest).await?;
    if meta.len() < 500 && url.to_lowercase().contains("image") {
        bail!("File too small (likely blocked) for {}", fixed_url);
    }
    Ok(())
}

pub async fn download_all_images(image_urls: Vec<String>, tmpdir: &Path) -> Result<Vec<String>> {
    download_all_images_with_progress(image_urls, tmpdir, None).await
}

pub async fn download_all_images_with_progress(
    image_urls: Vec<String>,
    tmpdir: &Path,
    progress: Option<ProgressCb>,
) -> Result<Vec<String>> {
    let mut paths: Vec<String> = Vec::new();
    let total = image_urls.len() as u64;
    for (i, url) in image_urls.iter().enumerate() {
        let mut fixed = url.clone().replace("\\u002F", "/");
        if fixed.starts_with("//") { fixed = format!("https:{}", fixed); }
        else if fixed.starts_with('/') { fixed = format!("https://www.tiktok.com{}", fixed); }
        let lower = fixed.to_lowercase();
        let ext = if lower.contains(".webp") { ".webp" } else if lower.contains(".png") { ".png" } else if lower.contains(".jpeg") { ".jpg" } else { ".jpg" };
        let dest = tmpdir.join(format!("img_{:03}{}", i, ext));
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..3 {
            match download_file_with_progress(&fixed, &dest, None).await {
                Ok(_) => { last_err = None; break; }
                Err(e) => {
                    last_err = Some(e);
                    if attempt < 2 {
                        tokio::time::sleep(std::time::Duration::from_millis(1500 * (attempt+1) as u64)).await;
                    }
                }
            }
        }
        if let Some(e) = last_err {
            return Err(e);
        }
        paths.push(dest.to_string_lossy().to_string());
        if let Some(cb) = &progress {
            cb((i + 1) as u64, total);
        }
    }
    Ok(paths)
}

pub async fn download_audio(music_url: &str, tmpdir: &Path) -> Option<String> {
    if music_url.is_empty() { return None; }
    let mut fixed = music_url.to_string();
    fixed = fixed.replace("\\u002F", "/");
    if fixed.starts_with("//") { fixed = format!("https:{}", fixed); }
    else if fixed.starts_with('/') {
        if fixed.starts_with("/video/") || fixed.starts_with("/music/") {
            fixed = format!("https://www.tikwm.com{}", fixed);
        } else {
            fixed = format!("https://www.tiktok.com{}", fixed);
        }
    }
    let lower = fixed.to_lowercase();
    let ext = if lower.contains(".mp3") { ".mp3" } else if lower.contains("mime_type") {
        if lower.contains("audio_mpeg") { ".mp3" } else if lower.contains("audio_mp4") { ".m4a" } else { ".m4a" }
    } else { ".m4a" };
    let dest = tmpdir.join(format!("audio{}", ext));
    let mut candidates = vec![fixed.clone()];
    if fixed.contains("tikwm.com") {

        let alt = fixed.replace("https://www.tikwm.com", "https://www.tiktok.com");
        if alt != fixed { candidates.push(alt); }

        let alt2 = fixed.replace("https://www.tikwm.com", "https://tikwm.com");
        if !candidates.contains(&alt2) { candidates.push(alt2); }
    } else if fixed.starts_with("https://www.tiktok.com/video/") {
        let alt = fixed.replace("https://www.tiktok.com", "https://www.tikwm.com");
        candidates.push(alt);
    }
    let mut failed_urls: Vec<String> = Vec::new();
    for attempt in 0..3 {
        let url_to_try = if attempt < candidates.len() { &candidates[attempt] } else { &fixed };
        if failed_urls.iter().any(|u| u == url_to_try) {
            if attempt >= 2 { break; }
            continue;
        }
        match download_file_with_progress(url_to_try, &dest, None).await {
            Ok(_) => {
                let meta = tokio::fs::metadata(&dest).await.ok()?;
                if meta.len() < 1000 {
                    failed_urls.push(url_to_try.to_string());
                    continue;
                }

                let (has_video, has_audio) = probe_streams(&dest);
                if has_video {
                    if !has_audio {

                        failed_urls.push(url_to_try.to_string());
                        continue;
                    }
                    let stripped = tmpdir.join("audio_stripped.m4a");
                    let out = Command::new("ffmpeg")
                        .args(["-y", "-i", dest.to_str().unwrap_or(""), "-vn", "-c:a", "copy", "-movflags", "+faststart"])
                        .arg(&stripped)
                        .output();
                    let ok = out.map(|o| o.status.success()).unwrap_or(false);
                    if ok {
                        if let Ok(sm) = tokio::fs::metadata(&stripped).await {
                            if sm.len() > 1000 {
                                let _ = tokio::fs::rename(&stripped, &dest).await;
                                return Some(dest.to_string_lossy().to_string());
                            }
                        }
                    }

                    return Some(dest.to_string_lossy().to_string());
                }
                return Some(dest.to_string_lossy().to_string());
            }
            Err(_) => {
                if attempt >= 2 { return None; }
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            }
        }
    }
    None
}

fn probe_streams(path: &Path) -> (bool, bool) {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", path.to_str().unwrap_or("")])
        .output()
        .ok();
    let mut has_video = false;
    let mut has_audio = false;
    if let Some(o) = out {
        if o.status.success() {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                match line.trim() {
                    "video" => has_video = true,
                    "audio" => has_audio = true,
                    _ => {}
                }
            }
        }
    }
    (has_video, has_audio)
}
