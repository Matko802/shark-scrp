use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;
use crate::presets::CompressProfile;
use crate::tools::{resolve, ToolId};

pub fn compress_audio(input: &Path, output: &Path, profile: &CompressProfile) -> Result<()> {
    let ffmpeg = resolve(ToolId::Ffmpeg).ok_or_else(|| anyhow::anyhow!("FFmpeg not found"))?;
    let (codec, ext_default) = match profile.audio_codec.as_str() {
        "libopus" => ("libopus", "opus"),
        "libmp3lame" => ("libmp3lame", "mp3"),
        "aac" => ("aac", "m4a"),
        "flac" => ("flac", "flac"),
        _ => ("libopus", "opus"),
    };
    let _ = ext_default;
    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-y", "-i"]);
    cmd.arg(input);
    cmd.args(["-vn", "-c:a", codec, "-b:a", &profile.audio_bitrate]);
    if profile.strip_metadata {
        cmd.args(["-map_metadata", "-1"]);
    }
    cmd.arg(output);
    let out = cmd.output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).lines().rev().take(3).collect::<Vec<_>>().join(" | ").chars().take(300).collect::<String>());
    }
    Ok(())
}
