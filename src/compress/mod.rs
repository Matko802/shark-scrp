pub mod image;
pub mod target;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CompressResult {
    pub input: PathBuf,
    pub output: PathBuf,
    pub before: u64,
    pub after: u64,
}

pub fn kind_of(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image-jpeg",
        "png" => "image-png",
        "gif" => "image-gif",
        "webp" | "avif" | "bmp" | "tiff" | "tif" | "svg" => "image-other",
        "mp4" | "mkv" | "webm" | "mov" | "avi" => "video",
        "mp3" | "m4a" | "aac" | "ogg" | "opus" | "wav" | "flac" => "audio",
        _ => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            if mime.type_() == mime_guess::mime::VIDEO { "video" }
            else if mime.type_() == mime_guess::mime::AUDIO { "audio" }
            else if mime.type_() == mime_guess::mime::IMAGE { "image-other" }
            else { "unknown" }
        }
    }
}

pub fn default_output_path_with_ext(input: &Path, output_dir: Option<&Path>, ext: &str) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let normalized = if ext.eq_ignore_ascii_case("jpeg") { "jpg".to_string() } else { ext.to_lowercase() };
    let name = format!("{}.compressed.{}", stem, normalized);
    if let Some(dir) = output_dir {
        dir.join(name)
    } else if let Some(parent) = input.parent() {
        if parent.as_os_str().is_empty() {
            PathBuf::from(name)
        } else {
            parent.join(name)
        }
    } else {
        PathBuf::from(name)
    }
}
