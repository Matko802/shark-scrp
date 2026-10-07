pub mod audio;
pub mod image;
pub mod video;

use anyhow::Result;
use std::path::{Path, PathBuf};
use crate::presets::CompressProfile;

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

pub fn default_output_path(input: &Path, output_dir: Option<&Path>) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("bin");
    let normalized_ext = if ext.eq_ignore_ascii_case("jpeg") { "jpg".to_string() } else { ext.to_lowercase() };
    let name = format!("{}.compressed.{}", stem, normalized_ext);
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

pub fn compress_one(input: &Path, output: &Path, profile: &CompressProfile) -> Result<CompressResult> {
    let before = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let kind = kind_of(input);
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    match kind {
        "image-jpeg" | "image-png" | "image-gif" | "image-other" => image::compress_image(input, output, profile)?,
        "video" => video::compress_video(input, output, profile)?,
        "audio" => audio::compress_audio(input, output, profile)?,
        _ => anyhow::bail!("Unsupported file type: {}", input.display()),
    }
    let after = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    Ok(CompressResult { input: input.to_path_buf(), output: output.to_path_buf(), before, after })
}
