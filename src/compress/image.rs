use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;
use crate::presets::CompressProfile;
use crate::tools::{resolve, ToolId};

fn run(cmd: &mut Command) -> Result<()> {
    let out = cmd.output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("tool failed").trim());
    }
    Ok(())
}

fn looks_valid_image(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.len() > 300).unwrap_or(false)
}

pub fn compress_image(input: &Path, output: &Path, profile: &CompressProfile) -> Result<()> {
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => compress_jpeg(input, output, profile),
        "png" => compress_png(input, output, profile),
        "gif" => compress_gif(input, output, profile),
        _ => compress_generic(input, output, profile),
    }
}

fn compress_jpeg(input: &Path, output: &Path, profile: &CompressProfile) -> Result<()> {
    if let Some(cjpeg) = resolve(ToolId::Cjpeg) {
        let tmp = output.with_extension("moz.jpg");
        let mut cmd = Command::new(&cjpeg);
        cmd.args(["-quality", &profile.jpeg_quality.to_string(), "-optimize", "-progressive", "-outfile"]);
        cmd.arg(&tmp);
        cmd.arg(input);
        if profile.strip_metadata {
            cmd.arg("-strip-all");
        }
        if run(cmd.borrow_mut()).is_ok() && looks_valid_image(&tmp) {
            let _ = std::fs::rename(&tmp, output);
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp);
    }
    compress_generic(input, output, profile)
}

fn compress_png(input: &Path, output: &Path, profile: &CompressProfile) -> Result<()> {
    let tmp = output.with_extension("ect.png");
    let copied = std::fs::copy(input, &tmp).is_ok();
    if copied {
        if let Some(ect) = resolve(ToolId::Ect) {
            let level = format!("-{}", profile.png_level.clamp(1, 9));
            let mut cmd = Command::new(&ect);
            cmd.args([level.as_str(), "--strict", "--mtime"]);
            if profile.strip_metadata {
                cmd.arg("--strip");
            }
            cmd.arg(&tmp);
            let _ = run(cmd.borrow_mut());
        }
        if looks_valid_image(&tmp) {
            let _ = std::fs::rename(&tmp, output);
            if profile.strip_metadata {
                let _ = strip_with_magick(output);
            }
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp);
    }
    compress_generic(input, output, profile)
}

fn compress_gif(input: &Path, output: &Path, profile: &CompressProfile) -> Result<()> {
    if let Some(gif) = resolve(ToolId::Gifsicle) {
        let mut cmd = Command::new(&gif);
        cmd.args(["-O3", "--no-warnings"]);
        cmd.arg(format!("--colors={}", profile.gif_colors.clamp(2, 256)));
        if profile.gif_lossy > 0 {
            cmd.arg(format!("--lossy={}", profile.gif_lossy.clamp(0, 200)));
        }
        cmd.arg(input);
        cmd.arg("-o");
        cmd.arg(output);
        if run(cmd.borrow_mut()).is_ok() && looks_valid_image(output) {
            return Ok(());
        }
    }
    compress_generic(input, output, profile)
}

fn compress_generic(input: &Path, output: &Path, profile: &CompressProfile) -> Result<()> {
    if let Some(magick) = resolve(ToolId::Magick) {
        let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let mut cmd = Command::new(&magick);
        if magick.ends_with("convert") {
            cmd.arg(input);
        } else {
            cmd.arg(input);
        }
        if profile.strip_metadata {
            cmd.arg("-strip");
        }
        match ext.as_str() {
            "jpg" | "jpeg" => {
                cmd.args(["-quality", &profile.jpeg_quality.to_string(), "-interlace", "Plane"]);
            }
            "png" => {
                cmd.args(["-quality", "92", "-define", "png:compression-level=9"]);
            }
            "gif" => {
                cmd.args(["-fuzz", "5%", "-layers", "Optimize"]);
            }
            "webp" => {
                cmd.args(["-quality", &profile.jpeg_quality.to_string()]);
            }
            "avif" => {
                cmd.args(["-quality", &profile.jpeg_quality.to_string()]);
            }
            _ => {
                cmd.args(["-quality", &profile.jpeg_quality.to_string()]);
            }
        }
        cmd.arg(output);
        if run(cmd.borrow_mut()).is_ok() && looks_valid_image(output) {
            return Ok(());
        }
    }
    bail!("No image backend available (install ImageMagick)")
}

fn strip_with_magick(path: &Path) -> Result<()> {
    if let Some(magick) = resolve(ToolId::Magick) {
        let tmp = path.with_extension("strip.tmp");
        let mut cmd = Command::new(&magick);
        cmd.arg(path);
        cmd.arg("-strip");
        cmd.arg(&tmp);
        if run(cmd.borrow_mut()).is_ok() && looks_valid_image(&tmp) {
            let _ = std::fs::rename(&tmp, path);
        }
    }
    Ok(())
}

trait BorrowMut {
    fn borrow_mut(&mut self) -> &mut Command;
}

impl BorrowMut for Command {
    fn borrow_mut(&mut self) -> &mut Command {
        self
    }
}
