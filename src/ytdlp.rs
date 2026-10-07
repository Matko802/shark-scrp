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

    if let Ok(path) = which::which(binary_name()) {
        return Some(path);
    }

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
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .user_agent("shark-scrp/1.0")
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        bail!("HTTP {} for {}", resp.status(), url);
    }
    let total = resp.content_length().unwrap_or(0);
    let _ = total;
    if let Some(parent) = dest.parent() { tokio::fs::create_dir_all(parent).await?; }
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(dest)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(dest, perm)?;
    }
    Ok(())
}

pub async fn ensure_ytdlp() -> Result<PathBuf> {

    if let Some(sys) = find_system_ytdlp() {
        return Ok(sys);
    }

    let cached = cached_ytdlp_path();
    if cached.exists() && is_executable(&cached) {
        if let Ok(out) = Command::new(&cached).arg("--version").output() {
            if out.status.success() {
                return Ok(cached);
            }
        }
        let _ = std::fs::remove_file(&cached);
    }

    let tag = match fetch_latest_tag().await {
        Ok(t) => t,
        Err(_) => "latest".to_string(),
    };
    let url = download_url_for_platform(&tag);

    let download_url = if tag == "latest" {

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

    if let Some(parent) = cache.parent() { std::fs::create_dir_all(parent)?; }

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

    if let Some(sys) = find_system_ytdlp() { return sys; }
    let cached = cached_ytdlp_path();
    if cached.exists() && is_executable(&cached) { return cached; }

    PathBuf::from("yt-dlp")
}

pub async fn ytdlp_path_async() -> PathBuf {
    match ensure_ytdlp().await {
        Ok(p) => p,
        Err(_) => PathBuf::from("yt-dlp"),
    }
}
