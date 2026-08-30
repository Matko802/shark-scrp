mod downloader;
mod extractor;
mod video;
mod ytdlp;

use anyhow::{bail, Result};
use clap::Parser;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn sanitize_filename(s: &str) -> String {
    // Replace invalid filesystem chars and control chars, trim, limit length
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => out.push('_'),
            c if c.is_control() => out.push('_'),
            _ => out.push(c),
        }
    }
    // Trim whitespace and dots, collapse multiple spaces
    let out = out.trim().trim_matches('.').to_string();
    // Collapse whitespace
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    // Limit by UTF-8 byte length (multi-byte chars like CJK count as several
    // bytes; filesystem name limits are byte-based, usually 255).
    // Keep room for the .mp4 suffix and yt-dlp's `.fdash-*.part` temp names.
    const MAX_BYTES: usize = 180;
    let mut truncated = String::new();
    for c in out.chars() {
        if truncated.len() + c.len_utf8() > MAX_BYTES {
            break;
        }
        truncated.push(c);
    }
    let truncated = truncated.trim().to_string();
    if truncated.is_empty() { "tiktok_slideshow".to_string() } else { truncated }
}

fn output_path_from_title(title: &str, fallback_id: Option<&str>, requested: &Option<PathBuf>) -> PathBuf {
    if let Some(p) = requested {
        // If user explicitly provided output, respect it (even if it's a directory, handle below)
        return if p.is_absolute() { p.clone() } else { std::env::current_dir().unwrap_or(PathBuf::from(".")).join(p) };
    }
    // No explicit output -> use title
    let base = if !title.trim().is_empty() {
        sanitize_filename(title)
    } else if let Some(id) = fallback_id {
        format!("tiktok_{}", id)
    } else {
        "tiktok_slideshow".to_string()
    };
    let filename = format!("{}.mp4", base);
    std::env::current_dir().unwrap_or(PathBuf::from(".")).join(filename)
}

fn extract_id_from_url(url: &str) -> Option<String> {
    // Instagram shortcode
    if extractor::is_instagram_url(url) {
        let re_ig = regex::Regex::new(r"/(?:p|reel|reels|tv)/([A-Za-z0-9_-]+)").ok()?;
        if let Some(caps) = re_ig.captures(url) {
            return caps.get(1).map(|m| m.as_str().to_string());
        }
    }
    // Try to find /photo/<digits> or /video/<digits> or last numeric segment
    let re = regex::Regex::new(r"/(?:photo|video)/(\d+)").ok()?;
    if let Some(caps) = re.captures(url) {
        return caps.get(1).map(|m| m.as_str().to_string());
    }
    // Fallback: last numeric sequence
    let re2 = regex::Regex::new(r"(\d{6,})").ok()?;
    re2.captures(url).and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

#[derive(Parser, Debug)]
#[command(name="shark-scrp", about="TikTok slideshow downloader → swipe video (TikTok-identical, no loop) - Rust", long_about=None, disable_version_flag = true)]
struct Args {
    /// TikTok slideshow URL (photo post)
    #[arg(required_unless_present_any = ["version", "yt_dlp_version"])]
    url: Option<String>,

    /// Print version (shark-scrp + yt-dlp)
    #[arg(short = 'v', long, action = clap::ArgAction::SetTrue)]
    version: bool,

    /// Output mp4 file (default: title of slideshow)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Video FPS (30 recommended for TikTok)
    #[arg(long, default_value_t=30)]
    fps: u32,

    /// Swipe transition duration in seconds (0.4-0.8 looks like TikTok)
    #[arg(long, default_value_t=0.6)]
    trans: f64,

    /// Seconds per image (auto = audio_duration / n if music exists, else 2.8)
    #[arg(long)]
    image_duration: Option<f64>,

    #[arg(long, default_value_t=1080)]
    width: u32,
    #[arg(long, default_value_t=1920)]
    height: u32,

    /// Keep temp images/audio
    #[arg(long)]
    keep_temp: bool,

    /// Ignore music, create silent video
    #[arg(long)]
    no_music: bool,

    /// Print yt-dlp version and exit (alias for -v)
    #[arg(long, hide = true)]
    yt_dlp_version: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.version || args.yt_dlp_version {
        if args.version {
            println!("shark-scrp {}", env!("CARGO_PKG_VERSION"));
        }
        let bin = ytdlp::ensure_ytdlp().await.unwrap_or_else(|_| ytdlp::ytdlp_command());
        let out = std::process::Command::new(&bin).arg("--version").output();
        match out {
            Ok(o) if o.status.success() => {
                println!("yt-dlp {}", String::from_utf8_lossy(&o.stdout).trim());
                std::process::exit(0);
            }
            Ok(o) => {
                eprint!("{}", String::from_utf8_lossy(&o.stderr));
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("yt-dlp not found: {}", e);
                std::process::exit(1);
            }
        }
    }
    // Ensure yt-dlp is available, fetching from web like GUI downloaders if not in PATH
    let _ = ytdlp::ensure_ytdlp().await;
    let url = args.url.as_ref().unwrap().trim().trim_matches('"').trim_matches('\'').to_string();
    let tmpdir = TempDir::new()?;
    let tmp_path = tmpdir.path().to_path_buf();
    let is_ig = extractor::is_instagram_url(&url);
    println!("{}", if is_ig { "Fetching Instagram post..." } else { "Fetching slideshow..." });
    println!("URL: {}", url);

    // Extract
    let resolved = extractor::resolve_url(&url).await;
    let info_url = if resolved != url {
        println!("Resolved: {}", resolved);
        resolved.clone()
    } else { url.clone() };

    let info = extractor::extract_info(&info_url).await?;
    if !info.title.is_empty() {
        println!("Title: {}", info.title);
    }
    if !info.is_video {
        println!("Found {} images • Music: {}", info.images.len(), if info.music_url.is_some() {"yes"} else {"no"});
    }
    // Use title to make filename if output not explicitly provided (simple)
    let fallback_id = extract_id_from_url(&info_url);
    let mut out_path = output_path_from_title(&info.title, fallback_id.as_deref(), &args.output);
    // If requested output is an existing directory, put title file inside it (use title, not directory name)
    if let Some(req) = &args.output {
        let req_path = if req.is_absolute() { req.clone() } else { std::env::current_dir().unwrap_or(PathBuf::from(".")).join(req) };
        if req_path.exists() && req_path.is_dir() {
            let base = if !info.title.trim().is_empty() {
                sanitize_filename(&info.title)
            } else if let Some(id) = &fallback_id {
                format!("tiktok_{}", id)
            } else {
                "tiktok_slideshow".to_string()
            };
            out_path = req_path.join(format!("{}.mp4", base));
        }
    }
    // Ensure .mp4 extension
    if out_path.extension().is_none() {
        out_path.set_extension("mp4");
    }
    println!("Output: {}", out_path.display());
    // Instagram reel / single video -> download directly (audio already muxed)
    if info.is_video {
        println!("Downloading video...");
        downloader::download_direct_video(&info_url, &out_path).await?;
        let d = video::ffprobe_duration(&out_path.to_string_lossy());
        println!("\nSaved to: {}", out_path.display());
        println!("Video • {:.1}s", d.unwrap_or(0.0));
        return Ok(());
    }
    if info.images.is_empty() {
        bail!("No images extracted. This may not be a photo slideshow. Try a URL like /photo/...");
    }
    if info.images.len() == 1 {
        println!("Note: Only 1 image found; swipe will be static");
    }

    // Download images
    let image_paths = downloader::download_all_images(info.images.clone(), &tmp_path).await?;

    // Download audio unless disabled
    let audio_path: Option<String> = if args.no_music {
        println!("Skipping music (--no-music)");
        None
    } else if let Some(music_url) = info.music_url {
        let dl = downloader::download_audio(&music_url, &tmp_path).await;
        if let Some(p) = &dl {
            if let Some(d) = video::ffprobe_duration(p) {
                println!("Audio: {:.1}s", d);
            }
        } else {
            println!("No audio, creating silent video");
        }
        dl
    } else {
        println!("No music found, creating silent video");
        None
    };

    // Create video - seamless swipe, fit aspect, no gaps
    println!("Creating video...");
    video::create_swipe_video(
        image_paths,
        audio_path.clone(),
        &out_path,
        args.fps,
        args.trans,
        args.image_duration,
        args.width,
        args.height,
        0,
        0,
    )?;

    println!("\nSaved to: {}", out_path.display());
    println!("{} images • {}x{} • {:.1}s", info.images.len(), args.width, args.height, video::ffprobe_duration(&out_path.to_string_lossy()).unwrap_or(0.0));

    if args.keep_temp {
        let keep_path = std::env::temp_dir().join(format!("tiktok_keep_{}", std::process::id()));
        std::fs::create_dir_all(&keep_path).ok();
        for entry in std::fs::read_dir(&tmp_path).unwrap_or_else(|_| std::fs::read_dir("/tmp").unwrap()) {
            if let Ok(e) = entry {
                let _ = std::fs::copy(e.path(), keep_path.join(e.file_name()));
            }
        }
        println!("Temp kept at {}", keep_path.display());
        std::mem::forget(tmpdir);
    }
    Ok(())
}
