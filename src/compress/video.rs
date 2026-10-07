use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;
use crate::presets::CompressProfile;
use crate::tools::{resolve, ToolId};

fn duration(path: &Path) -> Option<f64> {
    let ffprobe = resolve(ToolId::Ffprobe)?;
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok()
}

fn height_of(path: &Path) -> Option<u32> {
    let ffprobe = resolve(ToolId::Ffprobe)?;
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=height", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<u32>().ok()
}

pub fn compress_video(input: &Path, output: &Path, profile: &CompressProfile) -> Result<()> {
    let ffmpeg = resolve(ToolId::Ffmpeg).ok_or_else(|| anyhow::anyhow!("FFmpeg not found"))?;
    let src_h = height_of(input).unwrap_or(1080);
    let target_h = profile.video_max_height.min(src_h);
    let scale = if src_h > target_h {
        format!("scale=-2:{}", target_h)
    } else {
        "scale=iw:ih".to_string()
    };
    let (codec, extra): (&str, Vec<String>) = match profile.video_codec.as_str() {
        "libx265" => ("libx265", vec!["-tag:v".into(), "hvc1".into()]),
        "libvpx-vp9" => ("libvpx-vp9", vec!["-b:v".into(), "0".into()]),
        _ => ("libx264", vec![]),
    };
    let _ = duration(input);
    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-y", "-i"]);
    cmd.arg(input);
    cmd.args(["-vf", &scale, "-c:v", codec, "-crf", &profile.video_crf.to_string(), "-preset", &profile.video_preset]);
    cmd.args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"]);
    for e in &extra {
        cmd.arg(e);
    }
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
