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

    pub fn tiny() -> Self {
        Self {
            name: "Tiny".to_string(),
            jpeg_quality: 62,
            png_level: 9,
            gif_colors: 64,
            gif_lossy: 80,
            video_codec: "libx264".to_string(),
            video_crf: 28,
            video_preset: "fast".to_string(),
            video_max_height: 720,
            audio_codec: "libopus".to_string(),
            audio_bitrate: "96k".to_string(),
            strip_metadata: true,
        }
    }

    pub fn quality() -> Self {
        Self {
            name: "Quality".to_string(),
            jpeg_quality: 88,
            png_level: 2,
            gif_colors: 256,
            gif_lossy: 0,
            video_codec: "libx264".to_string(),
            video_crf: 19,
            video_preset: "slow".to_string(),
            video_max_height: 2160,
            audio_codec: "aac".to_string(),
            audio_bitrate: "192k".to_string(),
            strip_metadata: false,
        }
    }

    pub fn builtins() -> Vec<Self> {
        vec![Self::balanced(), Self::tiny(), Self::quality()]
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("shark-scrp").join("compress-presets.json"))
}

pub fn load_all() -> Vec<CompressProfile> {
    let mut out = CompressProfile::builtins();
    if let Some(p) = config_path() {
        if let Ok(data) = std::fs::read(&p) {
            if let Ok(custom) = serde_json::from_slice::<Vec<CompressProfile>>(&data) {
                for c in custom {
                    if !out.iter().any(|b| b.name == c.name) {
                        out.push(c);
                    }
                }
            }
        }
    }
    out
}

pub fn save_custom(profiles: &[CompressProfile]) {
    if let Some(p) = config_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let builtin_names = ["Balanced", "Tiny", "Quality"];
        let custom: Vec<_> = profiles.iter().filter(|x| !builtin_names.contains(&x.name.as_str())).collect();
        if let Ok(data) = serde_json::to_string_pretty(&custom) {
            let _ = std::fs::write(p, data);
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub output_dir: Option<PathBuf>,
    pub keep_originals: bool,
    pub parallel_jobs: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { output_dir: None, keep_originals: true, parallel_jobs: (num_cpus::get().max(2) - 1).min(4) }
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
        });
        let _ = std::fs::write(p, serde_json::to_string_pretty(&v).unwrap_or_default());
    }
}
