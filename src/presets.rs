use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressProfile {
    pub name: String,
    pub jpeg_quality: u8,
    pub png_level: u8,
    pub gif_colors: u32,
    pub gif_lossy: u32,
    pub video_codec: String,
    pub video_crf: u8,
    pub video_preset: String,
    pub video_max_height: u32,
    pub audio_codec: String,
    pub audio_bitrate: String,
    pub strip_metadata: bool,
}

impl CompressProfile {
    pub fn balanced() -> Self {
        Self {
            name: "Balanced".to_string(),
            jpeg_quality: 78,
            png_level: 3,
            gif_colors: 128,
            gif_lossy: 30,
            video_codec: "libx264".to_string(),
            video_crf: 23,
            video_preset: "medium".to_string(),
            video_max_height: 1080,
            audio_codec: "libopus".to_string(),
            audio_bitrate: "128k".to_string(),
            strip_metadata: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub output_dir: Option<PathBuf>,
    pub keep_originals: bool,
    pub parallel_jobs: usize,
    pub target_mb: f64,
    pub target_format: u32,
    pub target_effort: u32,
    pub remember_target: bool,
    pub download_mode: u32,
    pub transition_s: f64,
    pub fps: u32,
    pub no_music: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { output_dir: None, keep_originals: true, parallel_jobs: (num_cpus::get().max(2) - 1).min(4), target_mb: 25.0, target_format: 0, target_effort: 1, remember_target: false, download_mode: 0, transition_s: 0.6, fps: 30, no_music: false }
    }
}

fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("shark-scrp").join("settings.json"))
}

pub fn load_settings() -> AppSettings {
    if let Some(p) = settings_path() {
        if let Ok(data) = std::fs::read(&p) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&data) {
                return AppSettings {
                    output_dir: v.get("output_dir").and_then(|x| x.as_str()).map(PathBuf::from),
                    keep_originals: v.get("keep_originals").and_then(|x| x.as_bool()).unwrap_or(true),
                    parallel_jobs: v.get("parallel_jobs").and_then(|x| x.as_u64()).map(|n| (n as usize).clamp(1, 8)).unwrap_or_else(unwrap_or_default_config),
                    target_mb: v.get("target_mb").and_then(|x| x.as_f64()).map(|n| n.clamp(0.5, 2000.0)).unwrap_or(25.0),
                    target_format: v.get("target_format").and_then(|x| x.as_u64()).map(|n| (n as u32).min(8)).unwrap_or(0),
                    target_effort: v.get("target_effort").and_then(|x| x.as_u64()).map(|n| (n as u32).min(2)).unwrap_or(1),
                    remember_target: v.get("remember_target").and_then(|x| x.as_bool()).unwrap_or(false),
                    download_mode: v.get("download_mode").and_then(|x| x.as_u64()).map(|n| (n as u32).min(4)).unwrap_or(0),
                    transition_s: v.get("transition_s").and_then(|x| x.as_f64()).map(|n| n.clamp(0.2, 1.5)).unwrap_or(0.6),
                    fps: v.get("fps").and_then(|x| x.as_u64()).map(|n| (n as u32).clamp(15, 60)).unwrap_or(30),
                    no_music: v.get("no_music").and_then(|x| x.as_bool()).unwrap_or(false),
                };
            }
        }
    }
    AppSettings::default()
}

fn unwrap_or_default_config() -> usize {
    (num_cpus::get().max(2) - 1).min(4)
}

pub fn save_settings(s: &AppSettings) {
    if let Some(p) = settings_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let v = serde_json::json!({
            "output_dir": s.output_dir.as_ref().map(|x| x.to_string_lossy().to_string()),
            "keep_originals": s.keep_originals,
            "parallel_jobs": s.parallel_jobs,
            "target_mb": s.target_mb,
            "target_format": s.target_format,
            "target_effort": s.target_effort,
            "remember_target": s.remember_target,
            "download_mode": s.download_mode,
            "transition_s": s.transition_s,
            "fps": s.fps,
            "no_music": s.no_music,
        });
        let _ = std::fs::write(p, serde_json::to_string_pretty(&v).unwrap_or_default());
    }
}
