use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const YTDLP_REPO: &str = "yt-dlp/yt-dlp";
const YTDLP_API_LATEST: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";

fn cache_dir() -> PathBuf {
    if let Some(dir) = dirs::cache_dir() {
        dir.join("shark-scrp")
    } else if let Some(home) = dirs::home_dir() {
        home.join(".cache").join("shark-scrp")
    } else {
        std::env::temp_dir().join("shark-scrp-cache")
    }
}

fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn download_url_for_platform(tag: &str) -> String {
    // Use standalone binary if available, else generic python script
    // For Linux x86_64: yt-dlp_linux, aarch64: yt-dlp_linux_aarch64, macOS: yt-dlp_macos, windows: yt-dlp.exe
    let asset = if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else if cfg!(target_arch = "aarch64") && cfg!(target_os = "linux") {
        "yt-dlp_linux_aarch64"
    } else if cfg!(target_os = "linux") {
        "yt-dlp_linux"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp"
    };
    format!("https://github.com/{}/releases/download/{}/{}", YTDLP_REPO, tag, asset)
}

pub fn find_system_ytdlp() -> Option<PathBuf> {
    // Check PATH via `which` equivalent
    if let Ok(path) = which::which(binary_name()) {
        return Some(path);
    }
    // Also check common locations
    for p in ["/usr/bin/yt-dlp", "/usr/local/bin/yt-dlp", "/opt/homebrew/bin/yt-dlp"] {
        let pb = Path::new(p);
        if pb.exists() { return Some(pb.to_path_buf()); }
    }
    None
}

pub fn cached_ytdlp_path() -> PathBuf {
    cache_dir().join(binary_name())
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            return meta.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(windows)]
    { path.exists() }
}

async fn fetch_latest_tag() -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("shark-scrp/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client.get(YTDLP_API_LATEST)
        .header("Accept", "application/vnd.github+json")
        .send().await?;
    if !resp.status().is_success() {
        bail!("GitHub API failed: {}", resp.status());
    }
    let json: serde_json::Value = resp.json().await?;
    let tag = json.get("tag_name").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("no tag_name"))?;
    Ok(tag.to_string())
}

async fn download_file(url: &str, dest: &Path) -> Result<()> {
    use futures_util::StreamExt;
    use indicatif::{ProgressBar, ProgressStyle};
    use tokio::io::AsyncWriteExt;

    println!("Fetching yt-dlp from GitHub...");
    let client = reqwest::Client::builder()
        .user_agent("shark-scrp/1.0")
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        bail!("HTTP {} for {}", resp.status(), url);
    }
    let total = resp.content_length().unwrap_or(0);
    let pb = if total > 0 {
        let pb = ProgressBar::new(total);
        pb.set_style(ProgressStyle::default_bar().template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} {eta}").unwrap().progress_chars("█▉▊▋▌▍▎▏ "));
        pb.set_message("Fetching yt-dlp");
        Some(pb)
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_message("Fetching yt-dlp");
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(pb)
    };
    if let Some(parent) = dest.parent() { tokio::fs::create_dir_all(parent).await?; }
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if let Some(ref pb) = pb {
            if total > 0 { pb.set_position(downloaded); } else { pb.set_message(format!("Fetching yt-dlp {} bytes", downloaded)); }
        }
    }
    file.flush().await?;
    if let Some(pb) = pb { pb.finish_and_clear(); }
    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(dest)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(dest, perm)?;
    }
    println!("yt-dlp ready");
    Ok(())
}

pub async fn ensure_ytdlp() -> Result<PathBuf> {
    // 1. Check system PATH first (NixOS wraps ffmpeg+yt-dlp, so usually found)
    if let Some(sys) = find_system_ytdlp() {
        return Ok(sys);
    }
    // 2. Check cached binary
    let cached = cached_ytdlp_path();
    if cached.exists() && is_executable(&cached) {
        if let Ok(out) = Command::new(&cached).arg("--version").output() {
            if out.status.success() {
                return Ok(cached);
            }
        }
        let _ = std::fs::remove_file(&cached);
    }
    // 3. Fetch from web (like GUI downloaders)
    println!("yt-dlp not found, fetching from GitHub...");
    let tag = match fetch_latest_tag().await {
        Ok(t) => t,
        Err(_) => "latest".to_string(),
    };
    let url = download_url_for_platform(&tag);
    // If tag == "latest", the URL is https://github.com/yt-dlp/yt-dlp/releases/latest/download/<asset> which redirects
    // For specific tag, use that tag
    let download_url = if tag == "latest" {
        // Use latest redirect URL with asset name for latest
        let asset = if cfg!(target_os = "windows") { "yt-dlp.exe" }
            else if cfg!(target_arch = "aarch64") && cfg!(target_os = "linux") { "yt-dlp_linux_aarch64" }
            else if cfg!(target_os = "linux") { "yt-dlp_linux" }
            else if cfg!(target_os = "macos") { "yt-dlp_macos" }
            else { "yt-dlp" };
        format!("https://github.com/{}/releases/latest/download/{}", YTDLP_REPO, asset)
    } else {
        url
    };
    let cache = cached_ytdlp_path();
    // Ensure cache dir exists
    if let Some(parent) = cache.parent() { std::fs::create_dir_all(parent)?; }
    // Download with fallback: try standalone binary first, then generic script
    match download_file(&download_url, &cache).await {
        Ok(_) => Ok(cache),
        Err(_) => {
            let fallback = if tag == "latest" {
                format!("https://github.com/{}/releases/latest/download/yt-dlp", YTDLP_REPO)
            } else {
                format!("https://github.com/{}/releases/{}/download/yt-dlp", YTDLP_REPO, tag)
            };
            download_file(&fallback, &cache).await?;
            Ok(cache)
        }
    }
}

pub fn ytdlp_command() -> PathBuf {
    // Synchronous helper for extractor/downloader that need quick path without async fetch
    // Try system first, then cache, fallback to "yt-dlp" string (will fail but let caller handle)
    if let Some(sys) = find_system_ytdlp() { return sys; }
    let cached = cached_ytdlp_path();
    if cached.exists() && is_executable(&cached) { return cached; }
    // If not found, return "yt-dlp" and let ensure_ytdlp be called async elsewhere
    PathBuf::from("yt-dlp")
}

pub async fn ytdlp_path_async() -> PathBuf {
    match ensure_ytdlp().await {
        Ok(p) => p,
        Err(_) => PathBuf::from("yt-dlp"),
    }
}
