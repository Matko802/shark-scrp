use anyhow::{bail, Result};
use image::{imageops::FilterType, GenericImageView, Rgba, RgbaImage, Rgb, RgbImage};
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const W: u32 = 1080;
const H: u32 = 1920;
const FPS_DEFAULT: u32 = 30;
const TRANS_DEFAULT: f64 = 0.6;
const GAP: i32 = 0;
const RADIUS: u32 = 0;

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}
fn ease_tiktok(t: f64) -> f64 { ease_out_cubic(t) }

pub fn ffprobe_duration(path: &str) -> Option<f64> {
    let out = Command::new("ffprobe")
        .args(["-v","error","-show_entries","format=duration","-of","default=noprint_wrappers=1:nokey=1", path])
        .output().ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    s.parse::<f64>().ok()
}

fn prepare_cover(path: &str, width: u32, height: u32) -> Result<RgbImage> {
    // Fit: preserve aspect ratio, no cropping, centered on black background (simple)
    let img = image::open(path)?.to_rgb8();
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return Ok(RgbImage::from_pixel(width, height, Rgb([0,0,0])));
    }
    let scale = (width as f64 / iw as f64).min(height as f64 / ih as f64);
    let new_w = ((iw as f64 * scale).round() as u32).max(1);
    let new_h = ((ih as f64 * scale).round() as u32).max(1);
    let resized = image::imageops::resize(&img, new_w, new_h, FilterType::Lanczos3);
    // Create black canvas and paste centered
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([0,0,0]));
    let x = (width.saturating_sub(new_w)) / 2;
    let y = (height.saturating_sub(new_h)) / 2;
    // Manual paste (imageops::overlay would also work)
    for ry in 0..new_h {
        for rx in 0..new_w {
            let p = resized.get_pixel(rx, ry);
            canvas.put_pixel(x + rx, y + ry, *p);
        }
    }
    Ok(canvas)
}

fn rounded_image(img: &RgbImage, radius: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    // create mask L
    let mut mask = image::GrayImage::new(w, h);
    // draw rounded rectangle
    // Use imageproc? For simplicity we do manual: fill white where inside rounded rect
    // We'll use naïve pixel check
    for y in 0..h {
        for x in 0..w {
            let inside = is_inside_rounded(x, y, w, h, radius);
            mask.put_pixel(x, y, image::Luma([if inside { 255 } else { 0 }]));
        }
    }
    let mut rgba = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            let a = mask.get_pixel(x, y)[0];
            rgba.put_pixel(x, y, Rgba([p[0], p[1], p[2], a]));
        }
    }
    rgba
}

fn is_inside_rounded(x: u32, y: u32, w: u32, h: u32, r: u32) -> bool {
    if r == 0 { return true; }
    // Check corners
    // Top-left
    if x < r && y < r {
        let dx = r as i32 - x as i32 - 1;
        let dy = r as i32 - y as i32 - 1;
        return dx*dx + dy*dy <= (r as i32)*(r as i32);
    }
    if x >= w - r && y < r {
        let dx = x as i32 - (w - r) as i32;
        let dy = r as i32 - y as i32 - 1;
        return dx*dx + dy*dy <= (r as i32)*(r as i32);
    }
    if x < r && y >= h - r {
        let dx = r as i32 - x as i32 - 1;
        let dy = y as i32 - (h - r) as i32;
        return dx*dx + dy*dy <= (r as i32)*(r as i32);
    }
    if x >= w - r && y >= h - r {
        let dx = x as i32 - (w - r) as i32;
        let dy = y as i32 - (h - r) as i32;
        return dx*dx + dy*dy <= (r as i32)*(r as i32);
    }
    true
}

fn paste_rgba_onto(dst: &mut RgbaImage, src: &RgbaImage, off_x: i32, off_y: i32) {
    let (dw, dh) = dst.dimensions();
    let (sw, sh) = src.dimensions();
    for sy in 0..sh {
        let dy = sy as i32 + off_y;
        if dy < 0 || dy >= dh as i32 { continue; }
        for sx in 0..sw {
            let dx = sx as i32 + off_x;
            if dx < 0 || dx >= dw as i32 { continue; }
            let sp = src.get_pixel(sx, sy);
            if sp[3] == 0 { continue; }
            let dp = dst.get_pixel_mut(dx as u32, dy as u32);
            // alpha blend src over dst (dst is opaque black)
            let sa = sp[3] as f32 / 255.0;
            let da = 1.0 - sa;
            dp[0] = (sp[0] as f32 * sa + dp[0] as f32 * da).round() as u8;
            dp[1] = (sp[1] as f32 * sa + dp[1] as f32 * da).round() as u8;
            dp[2] = (sp[2] as f32 * sa + dp[2] as f32 * da).round() as u8;
            dp[3] = 255;
        }
    }
}

pub fn make_silent_video(
    image_paths: Vec<String>,
    output_path: &Path,
    fps: u32,
    trans_duration: f64,
    image_duration_opt: Option<f64>,
    total_duration_opt: Option<f64>,
    width: u32,
    height: u32,
    _gap: i32,
    _radius: u32,
) -> Result<(f64, f64, f64)> {
    let n = image_paths.len();
    if n == 0 { bail!("No images provided"); }
    let mut trans = trans_duration;
    if n == 1 { trans = 0.0; }

    let image_duration: f64;
    if let Some(td) = total_duration_opt {
        image_duration = td / n as f64;
    } else if let Some(id) = image_duration_opt {
        image_duration = id;
    } else {
        image_duration = 3.0;
    }
    let max_trans = image_duration * 0.8;
    if trans > max_trans {
        trans = max_trans;
        if trans < 0.15 { trans = 0.15; }
    }
    if trans < 0.0 { trans = 0.0; }
    let total_duration = n as f64 * image_duration;
    let total_frames = (total_duration * fps as f64).round();

    println!("Preparing {} images...", n);
    if let Some(parent) = output_path.parent() { std::fs::create_dir_all(parent)?; }

    // Smooth swipe: use raw pipe with easeOutCubic (TikTok curve) for buttery animation
    // xfade slideleft is linear and looks choppy; raw pipe gives true eased motion
    if trans <= 0.001 || n == 1 {
        // Single image or no transition: simple loop (efficient, no swipe needed)
        let dur = total_duration;
        let filter = format!("scale=w={}:h={}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps={}", width, height, width, height, fps);
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-y", "-loop", "1", "-t", &dur.to_string(), "-i", &image_paths[0],
                  "-vf", &filter,
                  "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", &fps.to_string(),
                  "-crf", "18", "-preset", "medium", "-movflags", "+faststart"]);
        cmd.arg(output_path);
        let out = cmd.output()?;
        if !out.status.success() {
            bail!("ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr));
        }
    } else {
        // Smooth eased slide via raw pipe (easeOutCubic) - ensures TikTok-identical curve
        return make_silent_video_raw(image_paths, output_path, fps, trans, Some(image_duration), Some(total_duration), width, height);
    }
    println!("Video ready ({:.1}s)", total_duration);
    Ok((total_duration, image_duration, trans))
}

fn make_silent_video_raw(
    image_paths: Vec<String>,
    output_path: &Path,
    fps: u32,
    trans_duration: f64,
    image_duration_opt: Option<f64>,
    total_duration_opt: Option<f64>,
    width: u32,
    height: u32,
) -> Result<(f64, f64, f64)> {
    // Fallback raw pipe (original, slower but reliable)
    let n = image_paths.len();
    let mut trans = trans_duration;
    if n == 1 { trans = 0.0; }
    let image_duration = if let Some(td) = total_duration_opt { td / n as f64 } else if let Some(id) = image_duration_opt { id } else { 3.0 };
    let max_trans = image_duration * 0.8;
    if trans > max_trans { trans = max_trans; if trans < 0.15 { trans = 0.15; } }
    let total_duration = n as f64 * image_duration;
    let total_frames = (total_duration * fps as f64).round() as usize;
    let mut covers: Vec<RgbImage> = Vec::with_capacity(n);
    for p in &image_paths {
        covers.push(prepare_cover(p, width, height)?);
    }
    let travel = width as i32;
    let mut child = Command::new("ffmpeg")
        .args(["-y","-f","rawvideo","-pixel_format","rgb24","-video_size",&format!("{}x{}", width, height),"-r",&fps.to_string(),"-i","-","-c:v","libx264","-pix_fmt","yuv420p","-crf","18","-preset","medium","-movflags","+faststart"])
        .arg(output_path).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
    let mut stdin = child.stdin.take().expect("ffmpeg stdin");
    let pb = ProgressBar::new(total_frames as u64);
    pb.set_style(ProgressStyle::default_bar().template("{msg} [{bar:40.cyan/blue}] {pos}/{len} {eta}").unwrap().progress_chars("█▉▊▋▌▍▎▏ "));
    pb.set_message("Rendering");
    for frame_idx in 0..total_frames {
        let t = frame_idx as f64 / fps as f64;
        let mut idx = (t / image_duration).floor() as usize;
        if idx >= n { idx = n - 1; }
        let residual = t - idx as f64 * image_duration;
        let frame: RgbImage;
        if idx == n - 1 {
            frame = covers[idx].clone();
        } else {
            let static_end = image_duration - trans;
            if residual < static_end || trans <= 0.001 {
                frame = covers[idx].clone();
            } else {
                let p = ((residual - static_end) / trans).clamp(0.0, 1.0);
                let eased = ease_tiktok(p);
                let offset = (eased * travel as f64).round() as i32;
                let mut canvas = RgbImage::from_pixel(width, height, Rgb([0,0,0]));
                // Simple slide without gap/radius (optimized)
                let out_x = -offset;
                let in_x = width as i32 - offset;
                // Paste covers directly (no rounded)
                for y in 0..height {
                    for x in 0..width {
                        let sx_out = x as i32 - out_x;
                        let sx_in = x as i32 - in_x;
                        let pixel = if sx_in >= 0 && sx_in < width as i32 {
                            covers[idx+1].get_pixel(sx_in as u32, y).clone()
                        } else if sx_out >= 0 && sx_out < width as i32 {
                            covers[idx].get_pixel(sx_out as u32, y).clone()
                        } else {
                            Rgb([0,0,0])
                        };
                        canvas.put_pixel(x, y, pixel);
                    }
                }
                frame = canvas;
            }
        }
        stdin.write_all(frame.as_raw())?;
        pb.inc(1);
    }
    pb.finish_and_clear();
    drop(stdin);
    let out = child.wait_with_output()?;
    if !out.status.success() { bail!("ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr)); }
    println!("Video ready ({:.1}s)", total_duration);
    Ok((total_duration, image_duration, trans))
}

pub fn mux_audio(silent_path: &Path, audio_path: &Path, output_path: &Path, shortest: bool) -> Result<()> {
    if !audio_path.exists() { bail!("Audio not found: {}", audio_path.display()); }
    if !silent_path.exists() { bail!("Silent video not found: {}", silent_path.display()); }
    if let Some(parent) = output_path.parent() { std::fs::create_dir_all(parent)?; }
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y").arg("-i").arg(silent_path).arg("-i").arg(audio_path)
        .arg("-map").arg("0:v:0").arg("-map").arg("1:a:0")
        .arg("-c:v").arg("copy").arg("-c:a").arg("aac").arg("-b:a").arg("192k");
    if shortest { cmd.arg("-shortest"); }
    cmd.arg("-movflags").arg("+faststart").arg(output_path);
    println!("Muxing audio...");
    let out = cmd.output()?;
    if !out.status.success() {
        bail!("ffmpeg mux failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

pub fn create_swipe_video(
    image_paths: Vec<String>,
    audio_path: Option<String>,
    output_path: &Path,
    fps: u32,
    trans_duration: f64,
    image_duration: Option<f64>,
    width: u32,
    height: u32,
    gap: i32,
    radius: u32,
) -> Result<()> {
    let audio_duration = if let Some(ref ap) = audio_path {
        if Path::new(ap).exists() {
            ffprobe_duration(ap)
        } else {
            println!("Audio missing, creating silent video");
            None
        }
    } else { None };

    let _ = audio_duration;

    let (total_opt, img_opt) = if let Some(d) = audio_duration {
        (Some(d), None)
    } else {
        (None, Some(image_duration.unwrap_or(2.8)))
    };

    let need_tmp = audio_duration.is_some() && audio_path.is_some();
    if need_tmp {
        let tmpdir = tempfile::tempdir()?;
        let silent_path = tmpdir.path().join("silent.mp4");
        let (total_used, _, _) = make_silent_video(image_paths.clone(), &silent_path, fps, trans_duration, img_opt, total_opt, width, height, gap, radius)?;
        let _ = total_used;
        let audio_p = Path::new(audio_path.as_ref().unwrap());
        mux_audio(&silent_path, audio_p, output_path, true)?;
        // tmpdir auto cleanup
    } else {
        make_silent_video(image_paths, output_path, fps, trans_duration, img_opt, total_opt, width, height, gap, radius)?;
        println!("Silent video ready");
    }
    Ok(())
}
