use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolId {
    Ffmpeg,
    Ffprobe,
    Magick,
    Gifsicle,
    Cjpeg,
    Ect,
    YtDlp,
}

impl ToolId {
    pub fn all() -> &'static [ToolId] {
        &[ToolId::Ffmpeg, ToolId::Ffprobe, ToolId::Magick, ToolId::Gifsicle, ToolId::Cjpeg, ToolId::Ect, ToolId::YtDlp]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ToolId::Ffmpeg => "FFmpeg",
            ToolId::Ffprobe => "FFprobe",
            ToolId::Magick => "ImageMagick",
            ToolId::Gifsicle => "Gifsicle",
            ToolId::Cjpeg => "MozJPEG",
            ToolId::Ect => "ECT",
            ToolId::YtDlp => "yt-dlp",
        }
    }

    pub fn role(&self) -> &'static str {
        match self {
            ToolId::Ffmpeg => "video + audio decode, process, encode",
            ToolId::Ffprobe => "media probing",
            ToolId::Magick => "image processing",
            ToolId::Gifsicle => "GIF optimization",
            ToolId::Cjpeg => "high-compression JPEGs",
            ToolId::Ect => "lossless PNG optimization",
            ToolId::YtDlp => "video / audio / slideshow download",
        }
    }

    pub fn candidates(&self) -> &'static [&'static str] {
        match self {
            ToolId::Ffmpeg => &["ffmpeg"],
            ToolId::Ffprobe => &["ffprobe"],
            ToolId::Magick => &["magick", "convert"],
            ToolId::Gifsicle => &["gifsicle"],
            ToolId::Cjpeg => &["cjpeg", "mozcjpeg"],
            ToolId::Ect => &["ect"],
            ToolId::YtDlp => &["yt-dlp"],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolStatus {
    pub id: ToolId,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
}

impl ToolStatus {
    pub fn available(&self) -> bool {
        self.path.is_some()
    }
}

fn version_of(bin: &str) -> Option<String> {
    let flag = if bin.contains("ect") { "--version" } else { "--version" };
    let out = std::process::Command::new(bin).arg(flag).output().ok()?;
    if !out.status.success() && out.stdout.is_empty() {
        return None;
    }
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    text.lines().next().map(|l| l.trim().chars().take(80).collect())
}

pub fn probe_all() -> Vec<ToolStatus> {
    ToolId::all().iter().map(|id| {
        let mut found: Option<PathBuf> = None;
        for c in id.candidates() {
            if let Ok(p) = which::which(c) {
                found = Some(p);
                break;
            }
        }
        if id == &ToolId::YtDlp && found.is_none() {
            let cached = crate::ytdlp::cached_ytdlp_path();
            if cached.exists() {
                found = Some(cached);
            }
        }
        let version = found.as_ref().and_then(|p| version_of(&p.to_string_lossy()));
        ToolStatus { id: *id, path: found, version }
    }).collect()
}

pub fn resolve(id: ToolId) -> Option<String> {
    for c in id.candidates() {
        if let Ok(p) = which::which(c) {
            return Some(p.to_string_lossy().to_string());
        }
    }
    if id == ToolId::YtDlp {
        let cached = crate::ytdlp::cached_ytdlp_path();
        if cached.exists() {
            return Some(cached.to_string_lossy().to_string());
        }
    }
    None
}
