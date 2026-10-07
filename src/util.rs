use std::path::PathBuf;

pub fn sanitize_filename(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => out.push('_'),
            c if c.is_control() => out.push('_'),
            _ => out.push(c),
        }
    }
    let out = out.trim().trim_matches('.').to_string();
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_BYTES: usize = 180;
    let mut truncated = String::new();
    for c in out.chars() {
        if truncated.len() + c.len_utf8() > MAX_BYTES {
            break;
        }
        truncated.push(c);
    }
    let truncated = truncated.trim().to_string();
    if truncated.is_empty() { "tiktok_slideshow".to_string() } else { truncated }
}

pub fn output_path_from_title(title: &str, fallback_id: Option<&str>, requested: &Option<PathBuf>, extension: &str) -> PathBuf {
    if let Some(p) = requested {
        let base = if p.is_absolute() { p.clone() } else { std::env::current_dir().unwrap_or(PathBuf::from(".")).join(p) };
        if base.exists() && base.is_dir() {
            let stem = if !title.trim().is_empty() {
                sanitize_filename(title)
            } else if let Some(id) = fallback_id {
                format!("tiktok_{}", id)
            } else {
                "tiktok_slideshow".to_string()
            };
            return base.join(format!("{}.{}", stem, extension));
        }
        return base;
    }
    let base = if !title.trim().is_empty() {
        sanitize_filename(title)
    } else if let Some(id) = fallback_id {
        format!("tiktok_{}", id)
    } else {
        "tiktok_slideshow".to_string()
    };
    std::env::current_dir().unwrap_or(PathBuf::from(".")).join(format!("{}.{}", base, extension))
}

pub fn extract_id_from_url(url: &str) -> Option<String> {
    let re_ig = regex::Regex::new(r"/(?:p|reel|reels|tv)/([A-Za-z0-9_-]+)").ok()?;
    if url.contains("instagram.com") || url.contains("instagr.am") {
        if let Some(caps) = re_ig.captures(url) {
            return caps.get(1).map(|m| m.as_str().to_string());
        }
    }
    let re = regex::Regex::new(r"/(?:photo|video)/(\d+)").ok()?;
    if let Some(caps) = re.captures(url) {
        return caps.get(1).map(|m| m.as_str().to_string());
    }
    let re2 = regex::Regex::new(r"(\d{6,})").ok()?;
    re2.captures(url).and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

pub fn format_bytes(n: u64) -> String {
    humansize::format_size(n, humansize::BINARY)
}

pub fn savings(before: u64, after: u64) -> String {
    if before == 0 {
        return String::new();
    }
    let pct = 100.0 * (1.0 - after as f64 / before as f64);
    format!("{:.0}%", pct)
}
