use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use crate::compress::{kind_of, CompressResult};
use crate::presets::CompressProfile;
use crate::tools::{resolve, ToolId};

pub const MAX_INPUT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const TARGET_PRESETS: &[(&str, f64)] = &[
    ("8 MB", 8.0),
    ("10 MB", 10.0),
    ("25 MB", 25.0),
    ("50 MB", 50.0),
    ("100 MB", 100.0),
    ("500 MB", 500.0),
    ("Custom", -1.0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    Fast,
    Balanced,
    Thorough,
}

impl Effort {
    pub fn all() -> &'static [&'static str] {
        &["Fast", "Balanced", "Thorough"]
    }
    pub fn from_index(i: u32) -> Self {
        match i {
            0 => Effort::Fast,
            2 => Effort::Thorough,
            _ => Effort::Balanced,
        }
    }
    fn ffmpeg_preset(self) -> &'static str {
        match self {
            Effort::Fast => "veryfast",
            Effort::Balanced => "medium",
            Effort::Thorough => "slow",
        }
    }
    fn cpu_used(self) -> &'static str {
        match self {
            Effort::Fast => "4",
            Effort::Balanced => "2",
            Effort::Thorough => "0",
        }
    }
    fn search_steps(self) -> usize {
        match self {
            Effort::Fast => 4,
            Effort::Balanced => 7,
            Effort::Thorough => 10,
        }
    }
    fn video_retries(self) -> usize {
        match self {
            Effort::Fast => 1,
            Effort::Balanced => 2,
            Effort::Thorough => 3,
        }
    }
    fn twopass(self) -> bool {
        matches!(self, Effort::Thorough)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFormat {
    Auto,
    Mp4,
    Webm,
    Mp3,
    Opus,
    Jpeg,
    Png,
    Webp,
    Gif,
}

impl OutFormat {
    pub fn all() -> &'static [&'static str] {
        &["Auto", "MP4", "WebM", "MP3", "Opus", "JPEG", "PNG", "WebP", "GIF"]
    }
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => OutFormat::Mp4,
            2 => OutFormat::Webm,
            3 => OutFormat::Mp3,
            4 => OutFormat::Opus,
            5 => OutFormat::Jpeg,
            6 => OutFormat::Png,
            7 => OutFormat::Webp,
            8 => OutFormat::Gif,
            _ => OutFormat::Auto,
        }
    }
    pub fn efficiency_hint(self) -> &'static str {
        match self {
            OutFormat::Auto => "Keep original format.",
            OutFormat::Mp4 => "Moderate efficiency. Compatible with everything.",
            OutFormat::Webm => "High efficiency. Smaller files, slower encode.",
            OutFormat::Mp3 => "Universal audio. Larger than Opus.",
            OutFormat::Opus => "Best audio efficiency. Modern players.",
            OutFormat::Jpeg => "Photos. Smaller than PNG.",
            OutFormat::Png => "Lossless. Large files.",
            OutFormat::Webp => "High efficiency images. Wide support.",
            OutFormat::Gif => "Animations. Large files, 256 colors.",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            OutFormat::Auto => "",
            OutFormat::Mp4 => "mp4",
            OutFormat::Webm => "webm",
            OutFormat::Mp3 => "mp3",
            OutFormat::Opus => "opus",
            OutFormat::Jpeg => "jpg",
            OutFormat::Png => "png",
            OutFormat::Webp => "webp",
            OutFormat::Gif => "gif",
        }
    }
    fn allows(self, kind: &str) -> bool {
        match kind {
            "image-jpeg" | "image-png" | "image-other" => matches!(self, OutFormat::Auto | OutFormat::Jpeg | OutFormat::Png | OutFormat::Webp),
            "image-gif" => matches!(self, OutFormat::Auto | OutFormat::Gif | OutFormat::Mp4 | OutFormat::Jpeg | OutFormat::Png | OutFormat::Webp),
            "video" => matches!(self, OutFormat::Auto | OutFormat::Mp4 | OutFormat::Webm | OutFormat::Mp3 | OutFormat::Opus),
            "audio" => matches!(self, OutFormat::Auto | OutFormat::Mp3 | OutFormat::Opus),
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TargetSpec {
    pub bytes: u64,
    pub format: OutFormat,
    pub effort: Effort,
}

pub type TargetProgress = Arc<dyn Fn(u64, u64) + Send + Sync>;

pub fn mb_to_bytes(mb: f64) -> u64 {
    ((mb * 1024.0 * 1024.0).round() as u64).max(1)
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn ffprobe_value(args: &[&str], path: &Path) -> Option<String> {
    let bin = resolve(ToolId::Ffprobe)?;
    let out = Command::new(bin).args(args).arg(path).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() || s == "N/A" {
        None
    } else {
        Some(s)
    }
}

fn probe_duration(path: &Path) -> Option<f64> {
    ffprobe_value(&["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"], path)?.parse::<f64>().ok()
}

fn probe_height(path: &Path) -> Option<u32> {
    ffprobe_value(&["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=height", "-of", "csv=p=0"], path)?.parse::<u32>().ok()
}

fn probe_has_audio(path: &Path) -> bool {
    ffprobe_value(&["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0"], path)
        .map(|s| s.lines().any(|l| l.trim() == "audio"))
        .unwrap_or(false)
}

fn ffmpeg_err(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).lines().rev().take(3).collect::<Vec<_>>().join(" | ").chars().take(240).collect()
}

fn base_profile(effort: Effort) -> CompressProfile {
    let mut p = CompressProfile::balanced();
    p.video_preset = effort.ffmpeg_preset().to_string();
    p.strip_metadata = true;
    p
}

pub fn compress_to_target(input: &Path, output: &Path, spec: &TargetSpec, progress: Option<TargetProgress>) -> Result<CompressResult> {
    let before = file_size(input);
    if before == 0 {
        bail!("Cannot read input file");
    }
    if before > MAX_INPUT_BYTES {
        bail!("File exceeds 2 GiB limit");
    }
    let kind = kind_of(input);
    if kind == "unknown" {
        bail!("Unsupported file type");
    }
    if !spec.format.allows(kind) {
        bail!("Format does not apply to this file");
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let step = |done: u64, total: u64| {
        if let Some(cb) = &progress {
            cb(done, total);
        }
    };
    if spec.format == OutFormat::Auto && before <= spec.bytes {
        if input == output {
            bail!("Input already fits, nothing to do");
        }
        std::fs::copy(input, output)?;
        step(1, 1);
        return Ok(CompressResult { input: input.to_path_buf(), output: output.to_path_buf(), before, after: file_size(output) });
    }
    let resolved_ext = if spec.format == OutFormat::Auto {
        let e = input.extension().and_then(|x| x.to_str()).unwrap_or("bin").to_lowercase();
        if e == "jpeg" { "jpg".to_string() } else { e }
    } else {
        spec.format.extension().to_string()
    };
    let probe = output.with_extension(format!("probe.{}", resolved_ext));
    let total = (spec.effort.search_steps() + 3) as u64;
    let mut done = 0u64;
    let mut tick = || {
        done += 1;
        step(done, total);
    };
    if kind.starts_with("image") {
        fit_image(input, &probe, output, spec, &mut tick)?;
    } else if kind == "video" {
        fit_video(input, &probe, output, spec, &mut tick)?;
    } else {
        fit_audio(input, &probe, output, spec, &mut tick)?;
    }
    let _ = std::fs::remove_file(&probe);
    let _ = std::fs::remove_file(output.with_extension("probe.passlog"));
    let after = file_size(output);
    if after == 0 {
        bail!("Encode produced no output");
    }
    Ok(CompressResult { input: input.to_path_buf(), output: output.to_path_buf(), before, after })
}

fn convert_to_format(input: &Path, tmp: &Path, ext: &str) -> Result<()> {
    let magick = resolve(ToolId::Magick).ok_or_else(|| anyhow::anyhow!("ImageMagick not found"))?;
    let out = Command::new(&magick).arg(input).arg("-strip").arg(tmp).output()?;
    if !out.status.success() {
        bail!("{}", ffmpeg_err(&out));
    }
    if file_size(tmp) < 300 {
        bail!("Conversion failed ({})", ext);
    }
    Ok(())
}

fn encode_image_at(input: &Path, tmp: &Path, ext: &str, quality: u8, colors: u32, lossy: u32, scale: f64) -> Result<()> {
    let mut profile = base_profile(Effort::Balanced);
    profile.jpeg_quality = quality;
    profile.gif_colors = colors;
    profile.gif_lossy = lossy;
    profile.png_level = 9;
    let mut src = input.to_path_buf();
    let owned;
    if (scale - 1.0).abs() > f64::EPSILON {
        let magick = resolve(ToolId::Magick).ok_or_else(|| anyhow::anyhow!("ImageMagick not found"))?;
        owned = tmp.with_extension(format!("scale.{}", ext));
        let pct = format!("{}%", (scale * 100.0).round());
        let out = Command::new(&magick).arg(input).arg("-strip").arg("-resize").arg(pct).arg(&owned).output()?;
        if !out.status.success() {
            bail!("{}", ffmpeg_err(&out));
        }
        src = owned;
    }
    let mut work = src.clone();
    if ext != input.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase() || (scale - 1.0).abs() > f64::EPSILON {
        let staged = tmp.with_extension(format!("cv.{}", ext));
        convert_to_format(&src, &staged, ext)?;
        work = staged;
    }
    super::image::compress_image(&work, tmp, &profile)?;
    if work != src && work != input.to_path_buf() {
        let _ = std::fs::remove_file(&work);
    }
    if src != input.to_path_buf() {
        let _ = std::fs::remove_file(&src);
    }
    Ok(())
}

fn fit_image(input: &Path, probe: &Path, output: &Path, spec: &TargetSpec, tick: &mut dyn FnMut()) -> Result<()> {
    let ext = if spec.format == OutFormat::Auto {
        let e = input.extension().and_then(|x| x.to_str()).unwrap_or("jpg").to_lowercase();
        if e == "jpeg" { "jpg".to_string() } else { e }
    } else {
        spec.format.extension().to_string()
    };
    if ext == "png" {
        return fit_png(input, probe, output, spec, tick);
    }
    if ext == "gif" {
        return fit_gif(input, probe, output, spec, tick);
    }
    let steps = spec.effort.search_steps();
    encode_image_at(input, probe, &ext, 90, 256, 0, 1.0)?;
    tick();
    if file_size(probe) <= spec.bytes {
        std::fs::rename(probe, output)?;
        return Ok(());
    }
    let mut lo: i32 = 25;
    let mut hi: i32 = 90;
    let mut best: Option<u8> = None;
    for _ in 0..steps {
        if lo > hi {
            break;
        }
        let mid = ((lo + hi) / 2) as u8;
        encode_image_at(input, probe, &ext, mid, 256, 0, 1.0)?;
        tick();
        if file_size(probe) <= spec.bytes {
            best = Some(mid);
            lo = mid as i32 + 1;
        } else {
            hi = mid as i32 - 1;
        }
    }
    match best {
        Some(q) => {
            encode_image_at(input, output, &ext, q, 256, 0, 1.0)?;
            tick();
            Ok(())
        }
        None => {
            encode_image_at(input, output, &ext, 25, 256, 0, 1.0)?;
            tick();
            if file_size(output) > spec.bytes {
                bail!("Cannot fit even at minimum quality");
            }
            Ok(())
        }
    }
}

fn ect_encode(input: &Path, tmp: &Path) -> Result<()> {
    std::fs::copy(input, tmp)?;
    if let Some(ect) = resolve(ToolId::Ect) {
        let _ = Command::new(&ect).args(["-9", "--strict", "--mtime", "--strip"]).arg(tmp).output();
    }
    Ok(())
}

fn fit_png(input: &Path, probe: &Path, output: &Path, spec: &TargetSpec, tick: &mut dyn FnMut()) -> Result<()> {
    let staged = probe.with_extension("stage.png");
    convert_to_format(input, &staged, "png")?;
    ect_encode(&staged, probe)?;
    tick();
    if file_size(probe) <= spec.bytes {
        let _ = std::fs::remove_file(&staged);
        std::fs::rename(probe, output)?;
        return Ok(());
    }
    let scales: &[f64] = match spec.effort {
        Effort::Fast => &[0.75, 0.5],
        Effort::Balanced => &[0.9, 0.75, 0.6, 0.45, 0.3],
        Effort::Thorough => &[0.95, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.25, 0.2],
    };
    let magick = resolve(ToolId::Magick).ok_or_else(|| anyhow::anyhow!("ImageMagick not found"))?;
    for s in scales.iter().take(spec.effort.search_steps()) {
        let pct = format!("{}%", (s * 100.0).round());
        let out = Command::new(&magick).arg(&staged).arg("-strip").arg("-resize").arg(pct).arg(probe).output()?;
        if !out.status.success() {
            continue;
        }
        ect_encode(probe, &probe.with_extension("e.png"))?;
        let _ = std::fs::rename(probe.with_extension("e.png"), probe);
        tick();
        if file_size(probe) <= spec.bytes {
            let _ = std::fs::remove_file(&staged);
            std::fs::rename(probe, output)?;
            return Ok(());
        }
    }
    let _ = std::fs::remove_file(&staged);
    bail!("Cannot fit PNG at this target size, try JPEG or WebP")
}

fn fit_gif(input: &Path, probe: &Path, output: &Path, spec: &TargetSpec, tick: &mut dyn FnMut()) -> Result<()> {
    let ladder: &[(u32, u32, f64)] = &[
        (256, 0, 1.0),
        (256, 30, 1.0),
        (128, 30, 1.0),
        (128, 80, 1.0),
        (64, 80, 1.0),
        (64, 150, 1.0),
        (64, 150, 0.75),
        (32, 150, 0.5),
        (32, 200, 0.35),
        (16, 200, 0.25),
    ];
    let take = spec.effort.search_steps().min(ladder.len());
    let mut applied = false;
    for (colors, lossy, scale) in ladder.iter().take(take) {
        encode_image_at(input, probe, "gif", 80, *colors, *lossy, *scale)?;
        tick();
        if file_size(probe) <= spec.bytes {
            applied = true;
            break;
        }
    }
    if !applied {
        let (c, l, s) = ladder[take - 1];
        encode_image_at(input, output, "gif", 80, c, l, s)?;
        tick();
        if file_size(output) > spec.bytes {
            bail!("Cannot fit GIF at this target size, try MP4");
        }
        return Ok(());
    }
    std::fs::rename(probe, output)?;
    Ok(())
}

fn audio_ladder(max_kbps: f64) -> Vec<u32> {
    [320, 256, 192, 160, 128, 96, 64, 48, 32].iter().cloned().filter(|k| *k as f64 <= max_kbps).collect()
}

fn encode_audio_at(input: &Path, output: &Path, codec: &str, kbps: u32, from_video: bool) -> Result<()> {
    let ffmpeg = resolve(ToolId::Ffmpeg).ok_or_else(|| anyhow::anyhow!("FFmpeg not found"))?;
    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-y", "-i"]).arg(input);
    if from_video {
        cmd.arg("-vn");
    }
    cmd.args(["-c:a", codec, "-b:a", &format!("{}k", kbps), "-map_metadata", "-1"]);
    cmd.arg(output);
    let out = cmd.output()?;
    if !out.status.success() {
        bail!("{}", ffmpeg_err(&out));
    }
    Ok(())
}

fn fit_audio(input: &Path, probe: &Path, output: &Path, spec: &TargetSpec, tick: &mut dyn FnMut()) -> Result<()> {
    let kind = kind_of(input);
    let from_video = kind == "video";
    let ext = if spec.format == OutFormat::Auto {
        input.extension().and_then(|x| x.to_str()).unwrap_or("opus").to_lowercase()
    } else {
        spec.format.extension().to_string()
    };
    let codec = match ext.as_str() {
        "mp3" => "libmp3lame",
        "m4a" | "aac" => "aac",
        "opus" | "ogg" => "libopus",
        _ => "libopus",
    };
    let dur = probe_duration(input).unwrap_or(0.0);
    if dur <= 0.0 {
        encode_audio_at(input, output, codec, 128, from_video)?;
        tick();
        return Ok(());
    }
    encode_audio_at(input, probe, codec, 192.min(320), from_video)?;
    tick();
    if file_size(probe) <= spec.bytes {
        std::fs::rename(probe, output)?;
        return Ok(());
    }
    let max_kbps = spec.bytes as f64 * 8.0 / 1000.0 / dur * 0.98;
    let ladder = audio_ladder(max_kbps);
    if ladder.is_empty() {
        bail!("Target too small for this audio length");
    }
    let pick = ladder[0].min(192);
    encode_audio_at(input, output, codec, pick, from_video)?;
    tick();
    if file_size(output) > spec.bytes {
        if ladder.len() > 1 {
            encode_audio_at(input, output, codec, ladder[ladder.len() - 1], from_video)?;
            tick();
        }
        if file_size(output) > spec.bytes {
            bail!("Cannot fit audio at this target size");
        }
    }
    Ok(())
}

fn video_bitrate_for_target(target: u64, dur: f64, audio_kbps: u64) -> Option<u64> {
    if dur <= 0.0 {
        return None;
    }
    let total_kbps = target as f64 * 8.0 / 1000.0 / dur * 0.98;
    let v = total_kbps - audio_kbps as f64;
    if v < 80.0 {
        None
    } else {
        Some(v.round() as u64)
    }
}

fn rez_ceiling(total_kbps: f64, src_h: u32) -> u32 {
    let ladder = if total_kbps < 500.0 {
        480
    } else if total_kbps < 1200.0 {
        720
    } else if total_kbps < 3000.0 {
        1080
    } else {
        2160
    };
    ladder.min(src_h).max(240)
}

fn encode_video_at(
    input: &Path,
    output: &Path,
    codec: &str,
    vbr_k: u64,
    max_h: u32,
    abrate_k: u64,
    acodec: &str,
    preset: &str,
    cpu_used: &str,
    twopass: bool,
) -> Result<()> {
    let ffmpeg = resolve(ToolId::Ffmpeg).ok_or_else(|| anyhow::anyhow!("FFmpeg not found"))?;
    let src_h = probe_height(input).unwrap_or(1080);
    let h = max_h.min(src_h);
    let mut vf: Vec<String> = Vec::new();
    if src_h > h {
        vf.push(format!("scale=-2:{}", h));
    }
    if twopass {
        let passlog = output.with_extension("passlog");
        let _ = std::fs::remove_file(&passlog);
        for pass in [1, 2] {
            let mut cmd = Command::new(&ffmpeg);
            cmd.args(["-y", "-i"]).arg(input);
            if !vf.is_empty() {
                cmd.args(["-vf", &vf.join(",")]);
            }
            if codec == "libvpx-vp9" {
                cmd.args(["-c:v", "libvpx-vp9", "-b:v", &format!("{}k", vbr_k), "-maxrate", &format!("{}k", (vbr_k as f64 * 1.5) as u64), "-bufsize", &format!("{}k", vbr_k * 2), "-quality", "good", "-cpu-used", cpu_used]);
            } else {
                cmd.args(["-c:v", codec, "-b:v", &format!("{}k", vbr_k), "-maxrate", &format!("{}k", (vbr_k as f64 * 1.5) as u64), "-bufsize", &format!("{}k", vbr_k * 2), "-preset", preset]);
            }
            cmd.args(["-pix_fmt", "yuv420p", "-pass", &pass.to_string(), "-passlogfile"]).arg(&passlog);
            if pass == 1 {
                cmd.args(["-an", "-f", "null", if cfg!(windows) { "NUL" } else { "/dev/null" }]);
            } else {
                if abrate_k > 0 {
                    cmd.args(["-c:a", acodec, "-b:a", &format!("{}k", abrate_k)]);
                } else {
                    cmd.arg("-an");
                }
                cmd.args(["-movflags", "+faststart", "-map_metadata", "-1"]);
                cmd.arg(output);
            }
            let out = cmd.output()?;
            if !out.status.success() {
                let _ = std::fs::remove_file(&passlog);
                let _ = std::fs::remove_file(output.with_extension("passlog-0.log"));
                bail!("{}", ffmpeg_err(&out));
            }
        }
        let _ = std::fs::remove_file(&passlog);
        let _ = std::fs::remove_file(output.with_extension("passlog-0.log"));
        return Ok(());
    }
    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-y", "-i"]).arg(input);
    if !vf.is_empty() {
        cmd.args(["-vf", &vf.join(",")]);
    }
    if codec == "libvpx-vp9" {
        cmd.args(["-c:v", "libvpx-vp9", "-b:v", &format!("{}k", vbr_k), "-maxrate", &format!("{}k", (vbr_k as f64 * 1.5) as u64), "-bufsize", &format!("{}k", vbr_k * 2), "-quality", "good", "-cpu-used", cpu_used]);
    } else {
        cmd.args(["-c:v", codec, "-b:v", &format!("{}k", vbr_k), "-maxrate", &format!("{}k", (vbr_k as f64 * 1.5) as u64), "-bufsize", &format!("{}k", vbr_k * 2), "-preset", preset]);
    }
    cmd.args(["-pix_fmt", "yuv420p"]);
    if abrate_k > 0 {
        cmd.args(["-c:a", acodec, "-b:a", &format!("{}k", abrate_k)]);
    } else {
        cmd.arg("-an");
    }
    cmd.args(["-movflags", "+faststart", "-map_metadata", "-1"]);
    cmd.arg(output);
    let out = cmd.output()?;
    if !out.status.success() {
        bail!("{}", ffmpeg_err(&out));
    }
    Ok(())
}

fn fit_video(input: &Path, probe: &Path, output: &Path, spec: &TargetSpec, tick: &mut dyn FnMut()) -> Result<()> {
    let (vcodec, acodec) = match spec.format {
        OutFormat::Webm => ("libvpx-vp9", "libopus"),
        OutFormat::Mp3 => ("", "libmp3lame"),
        OutFormat::Opus => ("", "libopus"),
        _ => ("libx264", "aac"),
    };
    if vcodec.is_empty() {
        return fit_audio(input, probe, output, spec, tick);
    }
    let has_audio = probe_has_audio(input);
    let abitrate: u64 = if has_audio { 128 } else { 0 };
    let src_h = probe_height(input).unwrap_or(1080);
    let preset = spec.effort.ffmpeg_preset();
    let cpu = spec.effort.cpu_used();
    let two = spec.effort.twopass();
    crf_video(input, probe, vcodec, 23, src_h.min(1080), abitrate, acodec, preset)?;
    tick();
    if file_size(probe) <= spec.bytes {
        std::fs::rename(probe, output)?;
        return Ok(());
    }
    let dur = probe_duration(input).unwrap_or(0.0);
    if dur <= 0.0 {
        bail!("Cannot probe video duration");
    }
    let total_kbps = spec.bytes as f64 * 8.0 / 1000.0 / dur;
    let ceiling = rez_ceiling(total_kbps, src_h);
    let mut vbr = match video_bitrate_for_target(spec.bytes, dur, abitrate) {
        Some(v) => v,
        None => bail!("Target too small for this video length"),
    };
    for _ in 0..spec.effort.video_retries() {
        encode_video_at(input, probe, vcodec, vbr, ceiling, abitrate, acodec, preset, cpu, two)?;
        tick();
        let got = file_size(probe);
        if got <= spec.bytes {
            std::fs::rename(probe, output)?;
            return Ok(());
        }
        vbr = ((vbr as f64 * spec.bytes as f64 / got as f64 * 0.97).round() as u64).max(80);
    }
    bail!("Cannot fit video at this target size, raise the target")
}

fn crf_video(input: &Path, output: &Path, codec: &str, crf: u8, max_h: u32, abrate: u64, acodec: &str, preset: &str) -> Result<()> {
    let ffmpeg = resolve(ToolId::Ffmpeg).ok_or_else(|| anyhow::anyhow!("FFmpeg not found"))?;
    let src_h = probe_height(input).unwrap_or(1080);
    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-y", "-i"]).arg(input);
    if src_h > max_h {
        cmd.args(["-vf", &format!("scale=-2:{}", max_h)]);
    }
    if codec == "libvpx-vp9" {
        cmd.args(["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", &crf.to_string(), "-cpu-used", "2"]);
    } else {
        cmd.args(["-c:v", codec, "-crf", &crf.to_string(), "-preset", preset]);
    }
    cmd.args(["-pix_fmt", "yuv420p"]);
    if abrate > 0 {
        cmd.args(["-c:a", acodec, "-b:a", &format!("{}k", abrate)]);
    } else {
        cmd.arg("-an");
    }
    cmd.args(["-movflags", "+faststart", "-map_metadata", "-1"]);
    cmd.arg(output);
    let out = cmd.output()?;
    if !out.status.success() {
        bail!("{}", ffmpeg_err(&out));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_fits_target() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.png");
        let mut img = image::RgbImage::new(1280, 720);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]);
        }
        img.save(&src).unwrap();
        let out = dir.path().join("out.compressed.jpg");
        let spec = TargetSpec { bytes: 150 * 1024, format: OutFormat::Jpeg, effort: Effort::Fast };
        let r = compress_to_target(&src, &out, &spec, None).unwrap();
        assert!(r.after <= spec.bytes, "after={} target={}", r.after, spec.bytes);
    }
    #[test]
    fn tiny_target_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.png");
        let mut img = image::RgbImage::new(1280, 720);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]);
        }
        img.save(&src).unwrap();
        let out = dir.path().join("out.compressed.jpg");
        let spec = TargetSpec { bytes: 1024, format: OutFormat::Jpeg, effort: Effort::Fast };
        assert!(compress_to_target(&src, &out, &spec, None).is_err());
    }
}
    #[test]
    fn video_fits_target() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.mp4");
        let st = std::process::Command::new("ffmpeg")
            .args(["-y", "-f", "lavfi", "-i", "testsrc=size=640x480:rate=30:duration=5", "-pix_fmt", "yuv420p"])
            .arg(&src)
            .status()
            .expect("ffmpeg missing");
        assert!(st.success());
        let out = dir.path().join("out.compressed.mp4");
        let spec = TargetSpec { bytes: 1024 * 1024, format: OutFormat::Mp4, effort: Effort::Fast };
        let r = compress_to_target(&src, &out, &spec, None).unwrap();
        assert!(r.after <= spec.bytes, "after={} target={}", r.after, spec.bytes);
    }
    #[test]
    fn video_bitrate_mode_fits_tight_target() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.mp4");
        let st = std::process::Command::new("ffmpeg")
            .args(["-y", "-f", "lavfi", "-i", "testsrc=size=640x480:rate=30:duration=5", "-pix_fmt", "yuv420p"])
            .arg(&src)
            .status()
            .expect("ffmpeg missing");
        assert!(st.success());
        let out = dir.path().join("out.compressed.mp4");
        let spec = TargetSpec { bytes: 60 * 1024, format: OutFormat::Mp4, effort: Effort::Balanced };
        let r = compress_to_target(&src, &out, &spec, None).unwrap();
        assert!(r.after <= spec.bytes, "after={} target={}", r.after, spec.bytes);
    }
