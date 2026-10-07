use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::Rng;
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use crate::ytdlp;

const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
const TIKWM_API: &str = "https://www.tikwm.com/api/";

#[derive(Debug, Clone)]
pub struct SlideInfo {
    pub images: Vec<String>,
    pub music_url: Option<String>,
    pub title: String,
    pub duration: Option<f64>,
    pub source: String,
    pub is_video: bool,
}

fn default_headers() -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("User-Agent".to_string(), DEFAULT_UA.to_string());
    h.insert("Referer".to_string(), "https://www.tiktok.com/".to_string());
    h.insert("Accept-Language".to_string(), "en-US,en;q=0.9".to_string());
    h.insert("Accept".to_string(), "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8".to_string());
    h
}

pub async fn resolve_url(url: &str) -> String {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::USER_AGENT, reqwest::header::HeaderValue::from_static(DEFAULT_UA));
    headers.insert(reqwest::header::REFERER, reqwest::header::HeaderValue::from_static("https://www.tiktok.com/"));

    if let Ok(resp) = client.head(url).headers(headers.clone()).send().await {
        let final_url = resp.url().to_string();
        if final_url != url && !final_url.is_empty() {
            return final_url;
        }
    }
    if let Ok(resp) = client.get(url).headers(headers).send().await {
        return resp.url().to_string();
    }
    url.to_string()
}

pub async fn extract_tikwm(url: &str) -> Option<SlideInfo> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;
    let payload = serde_json::json!({
        "url": url,
        "count": 12,
        "cursor": 0,
        "web": 1,
        "hd": 1
    });
    let res = client
        .post(TIKWM_API)
        .header("Content-Type", "application/json")
        .header("Origin", "https://www.tikwm.com")
        .header("Referer", "https://www.tikwm.com/")
        .header("User-Agent", DEFAULT_UA)
        .json(&payload)
        .send()
        .await
        .ok()?;
    let data: Value = res.json().await.ok()?;
    if data.get("code")?.as_i64()? != 0 {
        return None;
    }
    let inner = data.get("data")?.clone();

    let images_val = inner.get("images")?;
    let mut clean_images: Vec<String> = Vec::new();
    if let Some(arr) = images_val.as_array() {
        for img in arr {
            if let Some(s) = img.as_str() {
                clean_images.push(s.to_string());
            } else if let Some(obj) = img.as_object() {
                let u = obj.get("url")
                    .or(obj.get("download_url"))
                    .or(obj.get("display_url"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(u) = u {
                    clean_images.push(u);
                }
            }
        }
    }
    if clean_images.is_empty() {
        return None;
    }

    let mut music_url: Option<String> = None;

    if let Some(mi) = inner.get("music_info") {
        for key in ["play", "hdplay", "url", "downloadUrl", "originalUrl"] {
            if let Some(p) = mi.get(key).and_then(|v| v.as_str()) {
                if !p.is_empty() && p.starts_with("http") {
                    music_url = Some(p.to_string());
                    break;
                }
            }
        }

        if music_url.is_none() {
            if let Some(p) = mi.get("music").and_then(|v| v.get("play")).and_then(|v| v.as_str()) {
                if p.starts_with("http") { music_url = Some(p.to_string()); }
            }
        }
    }

    if music_url.is_none() {
        if let Some(p) = inner.get("hdplay").and_then(|v| v.as_str()) {
            if p.starts_with("http") { music_url = Some(p.to_string()); }
        }
    }
    if music_url.is_none() {
        if let Some(p) = inner.get("play").and_then(|v| v.as_str()) {
            if p.starts_with("http") { music_url = Some(p.to_string()); }
        }
    }

    if music_url.is_none() {
        if let Some(m) = inner.get("music").and_then(|v| v.as_str()) {
            if !m.is_empty() { music_url = Some(m.to_string()); }
        }
    }
    if music_url.is_none() {
        if let Some(m) = inner.get("music_url").and_then(|v| v.as_str()) {
            music_url = Some(m.to_string());
        }
    }
    if music_url.is_none() {
        if let Some(mi) = inner.get("music_info") {
            for key in ["play", "hdplay", "url", "downloadUrl"] {
                if let Some(p) = mi.get(key).and_then(|v| v.as_str()) {
                    if !p.is_empty() { music_url = Some(p.to_string()); break; }
                }
            }
        }
    }
    let title = inner.get("title").and_then(|v| v.as_str()).or(inner.get("desc").and_then(|v| v.as_str())).unwrap_or("").to_string();
    let duration = {
        let top = inner.get("duration").and_then(|v| v.as_f64()).or(inner.get("duration").and_then(|v| v.as_i64()).map(|i| i as f64));
        let music_dur = inner.get("music_info").and_then(|mi| mi.get("duration")).and_then(|v| v.as_f64()).or(inner.get("music_info").and_then(|mi| mi.get("duration")).and_then(|v| v.as_i64()).map(|i| i as f64));
        match (top, music_dur) {
            (Some(t), Some(md)) if t == 0.0 => Some(md),
            (Some(t), _) if t > 0.0 => Some(t),
            (None, Some(md)) => Some(md),
            (Some(t), None) => Some(t),
            _ => None,
        }
    };

    let music_url = music_url.map(|u| {
        let u = u.replace("\\u002F", "/");
        if u.starts_with("http://") || u.starts_with("https://") {
            u
        } else if u.starts_with("//") {
            format!("https:{}", u)
        } else if u.starts_with('/') {

            format!("https://www.tikwm.com{}", u)
        } else {
            u
        }
    });
    Some(SlideInfo {
        images: clean_images,
        music_url,
        title,
        duration,
        source: "tikwm".to_string(),
        is_video: false,
    })
}

fn extract_class_by_id(html: &str, id: &str) -> Option<String> {

    let pattern = format!(r#"<[^>]*\bid=["']{}["'][^>]*>"#, regex::escape(id));
    let re = Regex::new(&pattern).ok()?;
    let m = re.find(html)?;
    let tag = m.as_str();
    let re_class = Regex::new(r#"class=["']([^"']+)["']"#).ok()?;
    let caps = re_class.captures(tag)?;
    Some(caps[1].to_string())
}

fn solve_tiktok_challenge(html: &str) -> Option<(String, String, Option<String>, Option<String>)> {

    let cs_class = extract_class_by_id(html, "cs")?;

    let padded = format!("{}===", cs_class);

    let decoded = BASE64.decode(padded).or_else(|_| BASE64.decode(&cs_class)).ok()?;
    let mut challenge: Value = serde_json::from_slice(&decoded).ok()?;

    let v = challenge.get("v")?;
    let c_b64 = v.get("c")?.as_str()?;
    let expected_digest = BASE64.decode(c_b64).ok()?;

    let a_b64 = v.get("a")?.as_str()?;
    let base_bytes = BASE64.decode(a_b64).ok()?;
    let mut base_hasher = Sha256::new();
    base_hasher.update(&base_bytes);
    let base_hash_bytes = base_hasher.finalize_reset();

    let mut found: Option<String> = None;
    for i in 0..1_000_001 {
        let num_str = i.to_string();
        let mut hasher = Sha256::new();
        hasher.update(&base_bytes);
        hasher.update(num_str.as_bytes());
        let result = hasher.finalize();
        if result.as_slice() == expected_digest.as_slice() {
            found = Some(num_str);
            break;
        }

        let _ = base_hash_bytes;
    }
    let solution = found?;
    let d_b64 = BASE64.encode(solution.as_bytes());

    if let Some(obj) = challenge.as_object_mut() {
        obj.insert("d".to_string(), Value::String(d_b64));
    }
    let wci_json = serde_json::to_string(&challenge).ok()?;
    let wci_cookie_value = BASE64.encode(wci_json.as_bytes());

    let wci_cookie_name = extract_class_by_id(html, "wci")?;
    let rci_cookie_name = extract_class_by_id(html, "rci");
    let rci_cookie_value = extract_class_by_id(html, "rs");

    Some((wci_cookie_name, wci_cookie_value, rci_cookie_name, rci_cookie_value))
}

pub async fn extract_web(url: &str) -> Option<SlideInfo> {
    let resolved = resolve_url(url).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;
    let mut rng = rand::thread_rng();
    let ms_token: u64 = rng.gen_range(1_000_000_000_000_000_000..9_999_999_999_999_999_999);
    let cookie = format!("msToken={}; ttwid=1%7Cfake", ms_token);
    let res = client
        .get(&resolved)
        .header("User-Agent", DEFAULT_UA)
        .header("Referer", "https://www.tiktok.com/")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Cookie", cookie.clone())
        .send()
        .await
        .ok()?;
    let html = res.text().await.ok()?;

    let mut html_to_parse = html.clone();
    let mut extra_cookie: Option<String> = None;
    if html.contains("Please wait") || (html.to_lowercase().contains("challenge") && html.to_lowercase().contains("base64")) {
        if let Some((wci_name, wci_val, rci_name, rci_val)) = solve_tiktok_challenge(&html) {
            let mut cookie2 = cookie.clone();
            cookie2.push_str(&format!("; {}={}", wci_name, wci_val));
            if let (Some(rn), Some(rv)) = (rci_name, rci_val) {
                cookie2.push_str(&format!("; {}={}", rn, rv));
            }
            extra_cookie = Some(cookie2.clone());

            if let Ok(res2) = client
                .get(&resolved)
                .header("User-Agent", DEFAULT_UA)
                .header("Referer", "https://www.tiktok.com/")
                .header("Cookie", cookie2)
                .send()
                .await
            {
                if let Ok(html2) = res2.text().await {
                    html_to_parse = html2;
                }
            }
        } else {

        }
    }

    let mut images: Vec<String> = Vec::new();
    let mut music_url: Option<String> = None;
    let mut title: String = String::new();
    let mut duration: Option<f64> = None;

    fn walk(value: &Value, images: &mut Vec<String>, music_url: &mut Option<String>, title: &mut String, duration: &mut Option<f64>, depth: usize) {
        if depth > 30 { return; }
        match value {
            Value::Object(map) => {

                if let Some(ip) = map.get("imagePost") {
                    if let Some(obj) = ip.as_object() {
                        if let Some(imgs) = obj.get("images").and_then(|v| v.as_array()) {
                            for im in imgs {
                                let mut found: Option<String> = None;
                                if let Some(o) = im.as_object() {

                                    let candidates = [
                                        o.get("imageURL").and_then(|v| v.get("urlList")).and_then(|v| v.as_array()),
                                        o.get("imageURL").and_then(|v| v.get("url_list")).and_then(|v| v.as_array()),
                                        o.get("displayImage").and_then(|v| v.get("urlList")).and_then(|v| v.as_array()),
                                        o.get("urlList").and_then(|v| v.as_array()),
                                        o.get("url_list").and_then(|v| v.as_array()),
                                    ];
                                    for cand in candidates.iter().flatten() {
                                        if !cand.is_empty() {
                                            if let Some(u) = cand[0].as_str() {
                                                if u.starts_with("http") {
                                                    found = Some(u.to_string());
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    if found.is_none() {
                                        if let Some(u) = o.get("url").and_then(|v| v.as_str()) {
                                            found = Some(u.to_string());
                                        }
                                    }
                                } else if let Some(s) = im.as_str() {
                                    if s.starts_with("http") { found = Some(s.to_string()); }
                                }
                                if let Some(f) = found { images.push(f); }
                            }
                        }
                    }
                }

                if let Some(imgs) = map.get("images").and_then(|v| v.as_array()) {
                    if !imgs.is_empty() {
                        if let Some(first) = imgs.get(0) {
                            if let Some(obj) = first.as_object() {
                                if obj.contains_key("imageURL") || obj.contains_key("urlList") || obj.contains_key("url_list") {
                                    for im in imgs {
                                        if let Some(o) = im.as_object() {
                                            let cands = [
                                                o.get("imageURL").and_then(|v| v.get("urlList")).and_then(|v| v.as_array()),
                                                o.get("urlList").and_then(|v| v.as_array()),
                                                o.get("url_list").and_then(|v| v.as_array()),
                                            ];
                                            for cand in cands.iter().flatten() {
                                                if !cand.is_empty() {
                                                    if let Some(u) = cand[0].as_str() {
                                                        if u.starts_with("http") {
                                                            images.push(u.to_string());
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if music_url.is_none() {
                    if let Some(music) = map.get("music").and_then(|v| v.as_object()) {
                        for key in ["playUrl", "play_url", "downloadUrl", "url"] {
                            if let Some(v) = music.get(key) {
                                if let Some(s) = v.as_str() {
                                    if s.starts_with("http") {
                                        *music_url = Some(s.to_string());
                                        break;
                                    }
                                }
                                if let Some(obj) = v.as_object() {
                                    if let Some(list) = obj.get("urlList").or(obj.get("UrlList")).or(obj.get("url_list")).and_then(|x| x.as_array()) {
                                        if !list.is_empty() {
                                            if let Some(s) = list[0].as_str() {
                                                *music_url = Some(s.to_string());
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(arr) = v.as_array() {
                                    if !arr.is_empty() {
                                        if let Some(s) = arr[0].as_str() { *music_url = Some(s.to_string()); break; }
                                    }
                                }
                            }
                        }
                        if music_url.is_none() {
                            if let Some(pu) = music.get("playUrl") {
                                if let Some(obj) = pu.as_object() {
                                    if let Some(list) = obj.get("urlList").or(obj.get("UrlList")).and_then(|v| v.as_array()) {
                                        if !list.is_empty() {
                                            if let Some(s) = list[0].as_str() { *music_url = Some(s.to_string()); }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(desc) = map.get("desc").and_then(|v| v.as_str()) {
                    if !desc.is_empty() && (title.is_empty() || desc.len() > title.len()) {
                        *title = desc.to_string();
                    }
                }

                if duration.is_none() {
                    if let Some(d) = map.get("duration").and_then(|v| v.as_f64()) {
                        *duration = Some(d);
                    } else if let Some(d) = map.get("duration").and_then(|v| v.as_i64()) {
                        *duration = Some(d as f64);
                    }
                }
                for v in map.values() { walk(v, images, music_url, title, duration, depth+1); }
            }
            Value::Array(arr) => {
                for v in arr { walk(v, images, music_url, title, duration, depth+1); }
            }
            _ => {}
        }
    }

    let mut universal: Option<Value> = None;
    let re_universal = Regex::new(r#"<script[^>]+id="__UNIVERSAL_DATA_FOR_REHYDRATION__"[^>]*>(.*?)</script>"#).ok();
    if let Some(re) = re_universal {
        if let Some(caps) = re.captures(&html_to_parse) {
            if let Some(m) = caps.get(1) {
                if let Ok(j) = serde_json::from_str::<Value>(m.as_str()) {
                    universal = j.get("__DEFAULT_SCOPE__").cloned().or(Some(j));
                }
            }
        }
    }
    let mut sigi: Option<Value> = None;
    let re_sigi = Regex::new(r#"<script[^>]+id="SIGI_STATE"[^>]*>(.*?)</script>"#).ok();
    if let Some(re) = re_sigi {
        if let Some(caps) = re.captures(&html_to_parse) {
            if let Some(m) = caps.get(1) {
                if let Ok(j) = serde_json::from_str::<Value>(m.as_str()) {
                    sigi = Some(j);
                }
            }
        }
    }

    if let Some(u) = &universal {

        let mut wd: Option<Value> = None;
        if let Some(obj) = u.as_object() {
            for (k, v) in obj {
                if k.contains("video-detail") {
                    wd = Some(v.clone());
                    break;
                }
            }
            if wd.is_none() {
                if let Some(v) = obj.get("webapp.video-detail") { wd = Some(v.clone()); }
            }
        }
        if let Some(w) = wd.clone() {
            walk(&w, &mut images, &mut music_url, &mut title, &mut duration, 0);

            let item = w.get("itemInfo").and_then(|v| v.get("itemStruct")).cloned()
                .or(w.get("itemStruct").cloned());
            if let Some(it) = item { walk(&it, &mut images, &mut music_url, &mut title, &mut duration, 0); }
        }
        walk(u, &mut images, &mut music_url, &mut title, &mut duration, 0);
    }
    if let Some(s) = &sigi {
        if let Some(obj) = s.as_object() {
            if let Some(im) = obj.get("ItemModule").and_then(|v| v.as_object()) {
                for (_, val) in im { walk(val, &mut images, &mut music_url, &mut title, &mut duration, 0); }
            }
        }
        walk(s, &mut images, &mut music_url, &mut title, &mut duration, 0);
    }

    if let Some(u) = &universal {
        if let Some(wd) = u.get("webapp.video-detail").or(u.get("webapp.video-detail")) {

            if let Some(code) = wd.get("statusCode").and_then(|v| v.as_i64()) {
                if code == 10204 {

                    return None;
                }
                if code == 10216 || code == 10222 {

                    return None;
                }
            }
        }

        if let Some(obj) = u.as_object() {
            for (k,v) in obj {
                if k.contains("video-detail") {
                    if let Some(code) = v.get("statusCode").and_then(|x| x.as_i64()) {
                        if code == 10204 { return None; }
                    }
                }
            }
        }
    }

    if images.is_empty() {
        let re_url_list = Regex::new(r#""urlList"\s*:\s*\[(.*?)\]"#).ok()?;
        let mut cand_images: Vec<String> = Vec::new();
        for caps in re_url_list.captures_iter(&html_to_parse) {
            let block = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let re_url = Regex::new(r#""(https:[^"]+)""#).ok()?;
            for u_cap in re_url.captures_iter(block) {
                let u = u_cap.get(1).map(|m| m.as_str()).unwrap_or("").replace("\\u002F", "/");
                if u.contains(".mp4") || u.contains("playAddr") { continue; }
                if u.contains(".jpeg") || u.contains(".jpg") || u.contains(".webp") || u.contains(".png") || u.contains("image") || u.contains("obj/tos") {
                    cand_images.push(u);
                }
            }
        }

        let mut seen = std::collections::HashSet::new();
        let mut uniq: Vec<String> = Vec::new();
        for u in cand_images {
            if seen.insert(u.clone()) { uniq.push(u); }
        }
        if uniq.len() >= 2 && uniq.len() < 50 {
            if images.is_empty() {
                images = uniq.into_iter().take(35).collect();
            }
        }
    }
    if music_url.is_none() {
        let re_music = Regex::new(r#""playUrl"\s*:\s*"(https:[^"]+)""#).ok()?;
        if let Some(caps) = re_music.captures(&html_to_parse) {
            let u = caps.get(1).map(|m| m.as_str()).unwrap_or("").replace("\\u002F", "/");
            music_url = Some(u);
        } else {
            let re_music2 = Regex::new(r#""music"[^}]*"playUrl"\s*:\s*\{[^}]*"urlList"\s*:\s*\["(https:[^"]+)""#).ok()?;
            if let Some(caps) = re_music2.captures(&html_to_parse) {
                let u = caps.get(1).map(|m| m.as_str()).unwrap_or("").replace("\\u002F", "/");
                music_url = Some(u);
            }
        }
    }

    if let Some(ref mut m) = music_url {
        *m = m.replace("\\u002F", "/");
        if m.starts_with("//") {
            *m = format!("https:{}", m);
        } else if m.starts_with('/') && !m.starts_with("//") {

            if m.starts_with("/video/") || m.starts_with("/music/") {
                *m = format!("https://www.tikwm.com{}", m);
            } else {
                *m = format!("https://www.tiktok.com{}", m);
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    let mut uniq_images: Vec<String> = Vec::new();
    for u in images {
        let clean = u.replace("\\u002F", "/");
        if seen.insert(clean.clone()) { uniq_images.push(clean); }
    }

    for img in &mut uniq_images {
        if img.starts_with("//") { *img = format!("https:{}", img); }
    }

    if !uniq_images.is_empty() {
        return Some(SlideInfo {
            images: uniq_images,
            music_url,
            title,
            duration,
            source: "web".to_string(),
            is_video: false,
        });
    }
    let _ = extra_cookie;
    None
}

pub async fn extract_ytdlp(url: &str) -> Option<SlideInfo> {

    let ytdlp_url = if url.contains("/photo/") {
        url.replace("/photo/", "/video/")
    } else {
        url.to_string()
    };

    let ytdlp_bin = ytdlp::ytdlp_path_async().await;
    let output = tokio::process::Command::new(&ytdlp_bin)
        .arg("--dump-json")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg(&ytdlp_url)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        if stderr.contains("IP address is blocked") || stderr.contains("Unsupported URL") {
            eprintln!("{}", stderr.lines().next().unwrap_or("").trim());
        }
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() { return None; }
    let info: Value = serde_json::from_str(&stdout).ok()?;
    let mut music_url: Option<String> = None;

    if let Some(formats) = info.get("formats").and_then(|v| v.as_array()) {
        for f in formats {
            let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("");
            let acodec = f.get("acodec").and_then(|v| v.as_str()).unwrap_or("");

            if vcodec == "none" && acodec != "none" {
                if let Some(u) = f.get("url").and_then(|v| v.as_str()) {
                    if u.starts_with("http") {
                        music_url = Some(u.to_string());
                        break;
                    }
                }
            }
        }

        if music_url.is_none() {
            for f in formats {
                if let Some(u) = f.get("url").and_then(|v| v.as_str()) {
                    if u.contains("tiktokcdn") && u.contains("music") {
                        music_url = Some(u.to_string());
                        break;
                    }
                }
            }
        }
    }

    if music_url.is_none() {
        if let Some(u) = info.get("url").and_then(|v| v.as_str()) {
            if u.starts_with("http") { music_url = Some(u.to_string()); }
        }
    }
    let duration = info.get("duration").and_then(|v| v.as_f64())
        .or(info.get("duration").and_then(|v| v.as_i64()).map(|i| i as f64));
    let title = info.get("title").and_then(|v| v.as_str())
        .or(info.get("description").and_then(|v| v.as_str()))
        .unwrap_or("").to_string();

    let mut images: Vec<String> = Vec::new();
    if let Some(thumbs) = info.get("thumbnails").and_then(|v| v.as_array()) {
        for t in thumbs {
            if let Some(u) = t.get("url").and_then(|v| v.as_str()) {
                if u.starts_with("http") { images.push(u.to_string()); }
            }
        }
    }

    if music_url.is_none() && images.is_empty() { return None; }

    Some(SlideInfo {
        images,
        music_url,
        title,
        duration,
        source: "yt-dlp".to_string(),
        is_video: false,
    })
}

pub fn is_instagram_url(url: &str) -> bool {
    let host = url.to_lowercase();
    (host.contains("instagram.com") || host.contains("instagr.am"))
        && (host.contains("/p/") || host.contains("/reel") || host.contains("/tv/"))
}

fn is_ig_video_slide(e: &Value) -> bool {

    if let Some(mt) = e.get("media_type").and_then(|v| v.as_i64()) {
        if mt == 2 || mt == 5 {
            return true;
        }
    }
    e.get("video_versions")
        .and_then(|v| v.as_array())
        .map_or(false, |a| !a.is_empty())
}

fn collect_ig_audio(v: &Value, out: &mut Vec<String>, depth: usize) {
    if depth > 40 { return; }
    match v {
        Value::Object(map) => {

            if let Some(ao) = map.get("audio").and_then(|x| x.as_object()) {
                for key in ["audio_src", "audio_url", "download_audio_url"] {
                    if let Some(s) = ao.get(key).and_then(|x| x.as_str()) {
                        if s.starts_with("http") && !out.contains(&s.to_string()) {
                            out.push(s.to_string());
                        }
                    }
                }
            }
            for (k, val) in map {
                if let Some(s) = val.as_str() {
                    if s.starts_with("http") {
                        let kl = k.to_lowercase();
                        let audio_key = kl.contains("audio_src")
                            || kl.contains("audio_url")
                            || kl.contains("original_audio")
                            || kl.contains("music_asset")
                            || kl == "music"
                            || (kl.contains("download") && kl.contains("audio"));
                        if audio_key && !out.contains(&s.to_string()) {
                            out.push(s.to_string());
                        }
                    }
                } else if k == "music_info" || k == "clips_metadata" || k == "audio" || k == "music_asset_info" {
                    if let Some(m) = val.as_object() {
                        for (mk, mv) in m {
                            if let Some(s) = mv.as_str() {
                                let ml = mk.to_lowercase();
                                if s.starts_with("http")
                                    && (ml.contains("audio") || ml.contains("url") || ml.contains("src") || ml.contains("play"))
                                    && !out.contains(&s.to_string())
                                {
                                    out.push(s.to_string());
                                }
                            }
                        }
                    }
                }
                collect_ig_audio(val, out, depth + 1);
            }
        }
        Value::Array(arr) => {
            for x in arr { collect_ig_audio(x, out, depth + 1); }
        }
        _ => {}
    }
}

fn best_ig_video_url(e: &Value) -> Option<String> {
    if let Some(vv) = e.get("video_versions").and_then(|x| x.as_array()) {
        let mut best: Option<(u64, String)> = None;
        for f in vv {
            let url = f.get("url").and_then(|x| x.as_str());
            let w = f.get("width").and_then(|x| x.as_u64()).unwrap_or(0);
            let h = f.get("height").and_then(|x| x.as_u64()).unwrap_or(0);
            if let Some(u) = url {
                if !u.starts_with("http") { continue; }
                let score = w * h;
                if best.as_ref().map_or(true, |(s, _)| score > *s) {
                    best = Some((score, u.to_string()));
                }
            }
        }
        if let Some((_, u)) = best {
            return Some(u);
        }
    }
    None
}

fn best_ig_candidates(e: &Value) -> Option<String> {

    if let Some(cands) = e.get("image_versions2")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
    {
        let mut best: Option<(u64, String)> = None;
        for c in cands {
            let url = c.get("url").and_then(|v| v.as_str());
            let w = c.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
            let h = c.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
            if let Some(u) = url {
                let score = w * h;
                if best.as_ref().map_or(true, |(s, _)| score > *s) {
                    best = Some((score, u.to_string()));
                }
            }
        }
        if let Some((_, u)) = best {
            return Some(u);
        }
    }

    e.get("display_url")
        .or_else(|| e.get("display_src"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn walk_ig_tree(
    v: &Value,
    carousels: &mut Vec<Vec<Value>>,
    leaves: &mut Vec<Value>,
    captions: &mut Vec<String>,
    authors: &mut Vec<String>,
    durations: &mut Vec<f64>,
    depth: usize,
) {
    if depth > 50 {
        return;
    }
    match v {
        Value::Object(map) => {

            if let Some(cap) = map.get("caption").and_then(|x| x.as_object()) {
                if let Some(t) = cap.get("text").and_then(|x| x.as_str()) {
                    if !t.trim().is_empty() {
                        captions.push(t.trim().to_string());
                    }
                }
            }

            if let Some(d) = map.get("video_duration").and_then(|x| x.as_f64()) {
                durations.push(d);
            }
            if let Some(cm) = map.get("carousel_media").and_then(|x| x.as_array()) {
                if !cm.is_empty() {
                    carousels.push(cm.clone());
                    if let Some(u) = map.get("user").and_then(|x| x.as_object())
                        .and_then(|x| x.get("username")).and_then(|x| x.as_str()) {
                        authors.push(u.to_string());
                    }

                    return;
                }
            }
            if let Some(mt) = map.get("media_type").and_then(|x| x.as_i64()) {
                if (mt == 1 || mt == 2 || mt == 5 || mt == 8)
                    && (map.contains_key("image_versions2") || map.contains_key("video_versions"))
                {
                    if let Some(u) = map.get("user").and_then(|x| x.as_object())
                        .and_then(|x| x.get("username")).and_then(|x| x.as_str()) {
                        authors.push(u.to_string());
                    }
                    leaves.push(Value::Object(map.clone()));
                }
            }
            for val in map.values() {
                walk_ig_tree(val, carousels, leaves, captions, authors, durations, depth + 1);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                walk_ig_tree(val, carousels, leaves, captions, authors, durations, depth + 1);
            }
        }
        _ => {}
    }
}

pub async fn extract_instagram(url: &str) -> Result<SlideInfo> {

    let ytdlp_bin = ytdlp::ytdlp_path_async().await;
    let dump_dir = tempfile::tempdir()?;
    let output = tokio::process::Command::new(&ytdlp_bin)
        .args([
            "--write-pages",
            "--skip-download",
            "--ignore-errors",
            "--no-warnings",
        ])
        .arg(url)
        .current_dir(dump_dir.path())
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("failed to run yt-dlp: {}", e))?;
    let _ = output.status;

    let mut carousels: Vec<Vec<Value>> = Vec::new();
    let mut leaves: Vec<Value> = Vec::new();
    let mut captions: Vec<String> = Vec::new();
    let mut authors: Vec<String> = Vec::new();
    let mut durations: Vec<f64> = Vec::new();
    let mut parsed_dumps: Vec<Value> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dump_dir.path()) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".dump") && !name.ends_with(".html") {
                continue;
            }
            if let Ok(data) = std::fs::read(entry.path()) {
                if let Ok(v) = serde_json::from_slice::<Value>(&data) {
                    parsed_dumps.push(v.clone());
                    walk_ig_tree(
                        &v, &mut carousels, &mut leaves,
                        &mut captions, &mut authors, &mut durations, 0,
                    );
                }
            }
        }
    }

    let slides: Vec<Value> = if !carousels.is_empty() {
        carousels[0].clone()
    } else {
        leaves
            .iter()
            .filter(|m| {
                let mt = m.get("media_type").and_then(|x| x.as_i64()).unwrap_or(0);
                mt == 1 || mt == 2 || mt == 5
            })
            .cloned()
            .collect()
    };
    if slides.is_empty() {
        bail!("No extractable media for Instagram post");
    }

    let mut images: Vec<String> = Vec::new();
    let mut video_slides: usize = 0;
    for s in &slides {
        if is_ig_video_slide(s) {
            video_slides += 1;
        } else if let Some(u) = best_ig_candidates(s) {
            if !images.contains(&u) {
                images.push(u);
            }
        }
    }

    let title = if let Some(t) = captions.iter().max_by_key(|t| t.len()) {
        t.clone()
    } else if let Some(u) = authors.first() {
        format!("Post by {}", u)
    } else {
        String::new()
    };
    let duration = durations.into_iter().fold(0.0_f64, f64::max);

    let mut music_url: Option<String> = None;
    let mut audio_candidates: Vec<String> = Vec::new();
    for v in &parsed_dumps {
        collect_ig_audio(v, &mut audio_candidates, 0);
    }

    if let Some(u) = audio_candidates.iter().find(|u| u.starts_with("http")) {
        music_url = Some(u.clone());
    }

    if music_url.is_none() {
        for s in &slides {
            if is_ig_video_slide(s) {
                let muted = s.get("has_audio").and_then(|x| x.as_bool()) == Some(false);
                if !muted {
                    if let Some(u) = best_ig_video_url(s) {
                        music_url = Some(u);
                        break;
                    }
                }
            }
        }
    }

    if !images.is_empty() {
        return Ok(SlideInfo {
            images,
            music_url,
            title,
            duration: if duration > 0.0 { Some(duration) } else { None },
            source: "instagram".to_string(),
            is_video: false,
        });
    }
    if video_slides > 0 {
        if slides.len() == 1 {

            return Ok(SlideInfo {
                images: Vec::new(),
                music_url: None,
                title,
                duration: if duration > 0.0 { Some(duration) } else { None },
                source: "instagram".to_string(),
                is_video: true,
            });
        }
        bail!("Instagram multi-video carousel is not supported (use a single reel or photo post)");
    }
    bail!("No extractable media for Instagram post");
}

pub async fn extract_instagram_web(url: &str) -> Option<SlideInfo> {

    let resolved = resolve_url(url).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;
    let res = client
        .get(&resolved)
        .header("User-Agent", DEFAULT_UA)
        .header("Referer", "https://www.instagram.com/")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .ok()?;
    let html = res.text().await.ok()?;
    if html.contains("/accounts/login") {
        return None;
    }

    let mut images: Vec<String> = Vec::new();
    let re_display = Regex::new(r#""display_url"\s*:\s*"([^"]+)""#).ok()?;
    for caps in re_display.captures_iter(&html) {
        let u = caps.get(1).map(|m| m.as_str()).unwrap_or("")
            .replace("\\/", "/").replace("\\u002F", "/");
        if u.starts_with("http") && u.contains("cdninstagram.com") {
            if !images.contains(&u) {
                images.push(u);
            }
        }
    }
    if images.is_empty() {
        let re_og = Regex::new(r#"<meta\s+property=["']og:image["']\s+content=["']([^"']+)["']"#).ok()?;
        if let Some(c) = re_og.captures(&html) {
            let u = c.get(1).map(|m| m.as_str()).unwrap_or("").replace("&amp;", "&");
            if u.starts_with("http") {
                images.push(u);
            }
        }
    }
    if images.is_empty() {
        return None;
    }
    images.truncate(35);
    let title = {
        let re_desc = Regex::new(r#"<meta\s+property=["']og:description["']\s+content=["']([^"']*)["']"#).ok();
        if let Some(re) = re_desc {
            if let Some(c) = re.captures(&html) {
                let t = c.get(1).map(|m| m.as_str()).unwrap_or("").replace("&amp;", "&");
                if !t.trim().is_empty() {
                    t.trim().to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };
    Some(SlideInfo {
        images,
        music_url: None,
        title,
        duration: None,
        source: "instagram_web".to_string(),
        is_video: false,
    })
}

fn is_valid_music_url(u: &Option<String>) -> bool {
    if let Some(s) = u {
        s.starts_with("http") && (s.contains("tiktokcdn") || s.contains("muscdn") || s.contains("music") || s.contains("audio"))
    } else { false }
}

pub async fn extract_info(url: &str) -> Result<SlideInfo> {
    let original = url.trim();
    if !original.starts_with("http") {
        bail!("URL must start with http/https");
    }

    if is_instagram_url(original) {
        match extract_instagram(original).await {
            Ok(info) => return Ok(info),
            Err(err) => {
                if let Some(info) = extract_instagram_web(original).await {
                    return Ok(info);
                }
                bail!(
                    "Failed to extract Instagram post: {}\n\
                     - Post may be private/restricted, or Instagram blocked anonymous access (rate-limited).\n\
                     - Public photo posts, carousels, and reels are supported.\n\
                     URL: {}",
                    err,
                    original
                );
            }
        }
    }

    let tikwm_fut = extract_tikwm(original);
    let ytdlp_fut = extract_ytdlp(original);
    let web_fut = extract_web(original);
    let (tikwm_res, ytdlp_res, web_res) = tokio::join!(tikwm_fut, ytdlp_fut, web_fut);

    if let Some(mut res) = tikwm_res {
        if !res.images.is_empty() {
            if !is_valid_music_url(&res.music_url) {
                if let Some(yt) = &ytdlp_res {
                    if is_valid_music_url(&yt.music_url) {
                        res.music_url = yt.music_url.clone();
                        if res.duration.is_none() || res.duration == Some(0.0) {
                            res.duration = yt.duration;
                        }
                    }
                }
            }
            if (res.duration.is_none() || res.duration == Some(0.0)) {
                if let Some(yt) = &ytdlp_res {
                    if yt.duration.is_some() { res.duration = yt.duration; }
                }
            }
            return Ok(res);
        }
    }
    if let Some(mut res) = web_res {
        if !res.images.is_empty() {
            if !is_valid_music_url(&res.music_url) {
                if let Some(yt) = &ytdlp_res {
                    if is_valid_music_url(&yt.music_url) {
                        res.music_url = yt.music_url.clone();
                        if res.duration.is_none() || res.duration == Some(0.0) {
                            res.duration = yt.duration;
                        }
                    }
                }
            }
            if (res.duration.is_none() || res.duration == Some(0.0)) {
                if let Some(yt) = &ytdlp_res {
                    if yt.duration.is_some() { res.duration = yt.duration; }
                }
            }
            return Ok(res);
        }
    }

    if let Some(res) = ytdlp_res {
        if !res.images.is_empty() || res.music_url.is_some() {
            if !res.images.is_empty() {
                return Ok(res);
            }
        }
    }

    let resolved = resolve_url(original).await;
    if resolved != original {
        let tikwm2 = extract_tikwm(&resolved).await;
        let ytdlp2 = extract_ytdlp(&resolved).await;
        if let Some(mut res) = tikwm2 {
            if !res.images.is_empty() {
                if !is_valid_music_url(&res.music_url) {
                    if let Some(yt) = &ytdlp2 {
                        if is_valid_music_url(&yt.music_url) {
                            res.music_url = yt.music_url.clone();
                        }
                    }
                }
                return Ok(res);
            }
        }
        if let Some(mut res) = extract_web(&resolved).await {
            if !res.images.is_empty() {
                if !is_valid_music_url(&res.music_url) {
                    if let Some(yt) = &ytdlp2 {
                        if is_valid_music_url(&yt.music_url) {
                            res.music_url = yt.music_url.clone();
                        }
                    }
                }
                return Ok(res);
            }
        }
        if let Some(res) = ytdlp2 {
            if !res.images.is_empty() { return Ok(res); }
        }
    }
    bail!(
        "Failed to extract TikTok slideshow. Reasons:\n- URL may not be a photo slideshow (image post). Video URLs are not supported.\n- TikTok blocked extraction (WAF/IP block 10204). Try VPN or different network.\nURL: {}\nTip: Ensure URL is full TikTok photo URL like https://www.tiktok.com/@user/photo/123... or vm.tiktok.com/... short link.",
        original
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn carousel_dump() -> Value {

        json!({
            "data": {
                "xig_polaris_media": {
                    "if_not_gated_logged_out": {
                        "media_type": 8,
                        "caption": { "text": "A carousel with sound" },
                        "user": { "username": "testuser" },
                        "carousel_media": [
                            {
                                "media_type": 1,
                                "image_versions2": { "candidates": [
                                    { "url": "https://cdn/c1.jpg", "width": 400, "height": 400 }
                                ]}
                            },
                            {
                                "media_type": 2,
                                "image_versions2": { "candidates": [
                                    { "url": "https://cdn/c2.jpg", "width": 400, "height": 400 }
                                ]},
                                "video_versions": [
                                    { "url": "https://cdn/low.mp4", "width": 480, "height": 480 },
                                    { "url": "https://cdn/high.mp4", "width": 1080, "height": 1920 }
                                ],
                                "video_duration": 7.5,
                                "has_audio": true
                            }
                        ]
                    }
                }
            }
        })
    }

    #[test]
    fn audio_carousel_extracts_video_slide_audio() {
        let dump = carousel_dump();
        let mut carousels = Vec::new();
        let mut leaves = Vec::new();
        let mut captions = Vec::new();
        let mut authors = Vec::new();
        let mut durations = Vec::new();
        walk_ig_tree(&dump, &mut carousels, &mut leaves, &mut captions, &mut authors, &mut durations, 0);

        assert_eq!(carousels.len(), 1);
        let slides = &carousels[0];
        assert_eq!(slides.len(), 2);

        let mut candidates = Vec::new();
        collect_ig_audio(&dump, &mut candidates, 0);
        assert!(candidates.is_empty());

        let video_slide = &slides[1];
        assert!(is_ig_video_slide(video_slide));
        let url = best_ig_video_url(video_slide).unwrap();
        assert_eq!(url, "https://cdn/high.mp4");

        let mut music_url = candidates.into_iter().next();
        if music_url.is_none() {
            for s in slides {
                if is_ig_video_slide(s) && s.get("has_audio").and_then(|x| x.as_bool()) != Some(false) {
                    music_url = best_ig_video_url(s);
                    break;
                }
            }
        }
        assert_eq!(music_url.as_deref(), Some("https://cdn/high.mp4"));
    }

    #[test]
    fn audio_src_direct_field_is_preferred() {
        let dump = json!({
            "post": {
                "media_type": 2,
                "audio": { "audio_src": "https://cdn-preview/audio.mp4" },
                "video_versions": [ { "url": "https://cdn/movie.mp4", "width": 720, "height": 1280 } ]
            }
        });
        let mut candidates = Vec::new();
        collect_ig_audio(&dump, &mut candidates, 0);
        assert_eq!(candidates, vec!["https://cdn-preview/audio.mp4".to_string()]);
    }

    #[test]
    fn muted_video_slide_is_skipped() {
        let dump = json!({
            "carousel_media": [
                { "media_type": 1, "image_versions2": { "candidates": [{ "url": "https://cdn/p.jpg" }] } },
                { "media_type": 2, "has_audio": false, "video_versions": [{ "url": "https://cdn/silent.mp4", "width": 720, "height": 720 }] }
            ]
        });
        let mut carousels = Vec::new();
        let mut leaves = Vec::new();
        let mut captions = Vec::new();
        let mut authors = Vec::new();
        let mut durations = Vec::new();
        walk_ig_tree(&dump, &mut carousels, &mut leaves, &mut captions, &mut authors, &mut durations, 0);

        let mut music_url: Option<String> = None;
        for s in &carousels[0] {
            if is_ig_video_slide(s) {
                let muted = s.get("has_audio").and_then(|x| x.as_bool()) == Some(false);
                if !muted {
                    music_url = best_ig_video_url(s);
                    break;
                }
            }
        }
        assert!(music_url.is_none());
    }
}
