use anyhow::{bail, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use crate::ytdlp;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

async fn download_via_ytdlp(url: &str, dest: &Path) -> Result<()> {
    let dest_str = dest.to_string_lossy().to_string();
    let ytdlp_bin = ytdlp::ytdlp_path_async().await;
    let output = tokio::process::Command::new(&ytdlp_bin)
        .args(["--no-warnings", "--no-playlist", "-o", &dest_str, url])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("yt-dlp failed: {}", stderr.lines().next().unwrap_or("unknown"));
    }
    let meta = tokio::fs::metadata(dest).await?;
    if meta.len() < 500 {
        bail!("File too small");
    }
    Ok(())
}

pub async fn download_direct_video(url: &str, dest: &Path) -> Result<()> {
    // Download a whole post/reel as one MP4 (video + muxed audio) via yt-dlp
    download_via_ytdlp(url, dest).await
}

pub async fn download_file(url: &str, dest: &Path, desc: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let mut fixed_url = url.to_string();
    fixed_url = fixed_url.replace("\\u002F", "/");
    if fixed_url.starts_with("//") {
        fixed_url = format!("https:{}", fixed_url);
    } else if fixed_url.starts_with('/') {
        // Relative path like /video/music/... -> try tikwm host
        if fixed_url.starts_with("/video/") || fixed_url.starts_with("/music/") {
            fixed_url = format!("https://www.tikwm.com{}", fixed_url);
        } else {
            fixed_url = format!("https://www.tiktok.com{}", fixed_url);
        }
    }
    // Choose referer based on host
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
    let pb = if total > 0 {
        let pb = ProgressBar::new(total);
        pb.set_style(ProgressStyle::default_bar().template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} {eta}").unwrap().progress_chars("█▉▊▋▌▍▎▏ "));
        pb.set_message(desc.to_string());
        Some(pb)
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_message(desc.to_string());
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(pb)
    };
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if let Some(ref pb) = pb {
            if total > 0 { pb.set_position(downloaded); } else { pb.set_message(format!("{} {} bytes", desc, downloaded)); }
        }
    }
    file.flush().await?;
    if let Some(pb) = pb { pb.finish_and_clear(); }
    // verify size
    let meta = tokio::fs::metadata(dest).await?;
    if meta.len() < 500 && desc.starts_with("Image") {
        bail!("File too small (likely blocked) for {}", fixed_url);
    }
    Ok(())
}

pub async fn download_all_images(image_urls: Vec<String>, tmpdir: &Path) -> Result<Vec<String>> {
    println!("Downloading {} images...", image_urls.len());
    let mut paths: Vec<String> = Vec::new();
    for (i, url) in image_urls.iter().enumerate() {
        let mut fixed = url.clone().replace("\\u002F", "/");
        if fixed.starts_with("//") { fixed = format!("https:{}", fixed); }
        else if fixed.starts_with('/') { fixed = format!("https://www.tiktok.com{}", fixed); }
        let lower = fixed.to_lowercase();
        let ext = if lower.contains(".webp") { ".webp" } else if lower.contains(".png") { ".png" } else if lower.contains(".jpeg") { ".jpg" } else { ".jpg" };
        let dest = tmpdir.join(format!("img_{:03}{}", i, ext));
        let desc = format!("Image {}/{}", i+1, image_urls.len());
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..3 {
            match download_file(&fixed, &dest, &desc).await {
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
            eprintln!("Failed to download image {}: {}", i+1, e);
            return Err(e);
        }
        paths.push(dest.to_string_lossy().to_string());
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
    println!("Downloading audio...");
    // Build fallback list: try original, then alternative hosts if relative
    let mut candidates = vec![fixed.clone()];
    if fixed.contains("tikwm.com") {
        // Also try tiktok host as fallback
        let alt = fixed.replace("https://www.tikwm.com", "https://www.tiktok.com");
        if alt != fixed { candidates.push(alt); }
        // Also try without www
        let alt2 = fixed.replace("https://www.tikwm.com", "https://tikwm.com");
        if !candidates.contains(&alt2) { candidates.push(alt2); }
    } else if fixed.starts_with("https://www.tiktok.com/video/") {
        let alt = fixed.replace("https://www.tiktok.com", "https://www.tikwm.com");
        candidates.push(alt);
    }
    for attempt in 0..3 {
        let url_to_try = if attempt < candidates.len() { &candidates[attempt] } else { &fixed };
        match download_file(url_to_try, &dest, "Audio").await {
            Ok(_) => {
                let meta = tokio::fs::metadata(&dest).await.ok()?;
                if meta.len() < 1000 {
                    continue;
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
