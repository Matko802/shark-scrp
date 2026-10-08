use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::presets::AppSettings;

enum Msg {
    Stage(String, f64),
    Done(Result<String, String>),
}

type Queue = Arc<Mutex<VecDeque<Msg>>>;

fn push(q: &Queue, m: Msg) {
    q.lock().unwrap().push_back(m);
}

pub fn build_download_page(settings: Rc<RefCell<AppSettings>>) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    let url_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some("Paste URL..."));
    entry.set_hexpand(true);
    let paste_btn = gtk4::Button::with_label("Paste");
    let go_btn = gtk4::Button::with_label("Download");
    go_btn.add_css_class("suggested-action");
    url_row.append(&entry);
    url_row.append(&paste_btn);
    url_row.append(&go_btn);
    root.append(&url_row);

    let opt_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let modes = ["Auto", "TikTok images only", "TikTok slideshow video", "Best video (yt-dlp)", "Audio only (yt-dlp)"];
    let mode_drop = gtk4::DropDown::from_strings(&modes);
    mode_drop.set_selected(settings.borrow().download_mode);
    let open_btn = gtk4::Button::with_label("Open folder");
    opt_row.append(&gtk4::Label::new(Some("Mode:")));
    opt_row.append(&mode_drop);
    opt_row.append(&open_btn);
    root.append(&opt_row);

    let adv = gtk4::Expander::new(Some("Video options (slideshow)"));
    let adv_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    let trans = gtk4::SpinButton::with_range(0.2, 1.5, 0.1);
    trans.set_value(settings.borrow().transition_s);
    let fps = gtk4::SpinButton::with_range(15.0, 60.0, 1.0);
    fps.set_value(settings.borrow().fps as f64);
    let no_music = gtk4::CheckButton::with_label("No music");
    no_music.set_active(settings.borrow().no_music);
    adv_box.append(&gtk4::Label::new(Some("Transition s:")));
    adv_box.append(&trans);
    adv_box.append(&gtk4::Label::new(Some("FPS:")));
    adv_box.append(&fps);
    adv_box.append(&no_music);
    adv.set_child(Some(&adv_box));
    root.append(&adv);

    let status = gtk4::Label::new(Some("Idle"));
    status.set_xalign(0.0);
    status.set_wrap(true);
    root.append(&status);

    let bar = gtk4::ProgressBar::new();
    bar.set_show_text(true);
    root.append(&bar);

    let out_label = gtk4::Label::new(Some("Output: current folder"));
    out_label.set_xalign(0.0);
    out_label.add_css_class("dim-label");
    root.append(&out_label);

    let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));

    {
        let entry_c = entry.clone();
        paste_btn.connect_clicked(move |_| {
            if let Some(display) = gtk4::gdk::Display::default() {
                let clipboard = display.clipboard();
                let entry_cc = entry_c.clone();
                glib::MainContext::default().spawn_local(async move {
                    if let Ok(text) = clipboard.read_text_future().await {
                        if let Some(t) = text {
                            entry_cc.set_text(t.trim());
                        }
                    }
                });
            }
        });
    }

    {
        let sc = settings.clone();
        mode_drop.connect_selected_notify(move |d| {
            sc.borrow_mut().download_mode = d.selected();
            crate::presets::save_settings(&sc.borrow());
        });
    }
    {
        let sc = settings.clone();
        trans.connect_value_changed(move |s| {
            sc.borrow_mut().transition_s = s.value().clamp(0.2, 1.5);
            crate::presets::save_settings(&sc.borrow());
        });
    }
    {
        let sc = settings.clone();
        fps.connect_value_changed(move |s| {
            sc.borrow_mut().fps = (s.value() as u32).clamp(15, 60);
            crate::presets::save_settings(&sc.borrow());
        });
    }
    {
        let sc = settings.clone();
        no_music.connect_toggled(move |c| {
            sc.borrow_mut().no_music = c.is_active();
            crate::presets::save_settings(&sc.borrow());
        });
    }

    {
        let settings_c = settings.clone();
        open_btn.connect_clicked(move |_| {
            let dir = settings_c.borrow().output_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let _ = open::that(dir);
        });
    }

    {
        let status_c = status.clone();
        let bar_c = bar.clone();
        let queue_c = queue.clone();
        let go_c = go_btn.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
            let msgs: Vec<Msg> = {
                let mut q = queue_c.lock().unwrap();
                q.drain(..).collect()
            };
            for msg in msgs {
                match msg {
                    Msg::Stage(text, frac) => {
                        status_c.set_text(&text);
                        bar_c.set_fraction(frac);
                        bar_c.set_text(Some(&format!("{:.0}%", frac * 100.0)));
                    }
                    Msg::Done(res) => {
                        go_c.set_sensitive(true);
                        match res {
                            Ok(path) => {
                                status_c.set_text(&format!("Saved to: {}", path));
                                bar_c.set_fraction(1.0);
                                bar_c.set_text(Some("Done"));
                            }
                            Err(e) => {
                                status_c.set_text(&format!("Failed: {}", e));
                                bar_c.set_fraction(0.0);
                                bar_c.set_text(Some("Failed"));
                            }
                        }
                    }
                }
            }
            glib::ControlFlow::Continue
        });
        let _ = bar;
    }

    match settings.borrow().output_dir.clone() {
        Some(dir) => out_label.set_text(&format!("Output: {}", dir.display())),
        None => out_label.set_text("Output: current folder (change in Settings)"),
    }
    {
        let settings_c = settings.clone();
        let out_label_c = out_label.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let text = match settings_c.borrow().output_dir.clone() {
                Some(dir) => format!("Output: {}", dir.display()),
                None => "Output: current folder (change in Settings)".to_string(),
            };
            if out_label_c.text().as_str() != text {
                out_label_c.set_text(&text);
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let entry_c = entry.clone();
        let mode_c = mode_drop.clone();
        let trans_c = trans.clone();
        let fps_c = fps.clone();
        let no_music_c = no_music.clone();
        let settings_c = settings.clone();
        let queue_c = queue.clone();
        let status_c = status.clone();
        go_btn.connect_clicked(move |btn| {
            let url = entry_c.text().to_string().trim().trim_matches('"').trim_matches('\'').to_string();
            if url.is_empty() {
                status_c.set_text("Paste a URL first.");
                return;
            }
            btn.set_sensitive(false);
            let mode = mode_c.selected();
            let trans_v = trans_c.value();
            let fps_v = fps_c.value() as u32;
            let silent = no_music_c.is_active();
            let out_dir = settings_c.borrow().output_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let queue_cc = queue_c.clone();
            push(&queue_cc, Msg::Stage("Starting...".to_string(), 0.02));
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
                let res = match rt {
                    Ok(rt) => rt.block_on(download_flow(&url, mode, trans_v, fps_v, silent, &out_dir, &queue_cc)),
                    Err(e) => Err(format!("runtime: {}", e)),
                };
                push(&queue_cc, Msg::Done(res));
            });
        });
    }

    root.into()
}

async fn download_flow(
    url: &str,
    mode: u32,
    trans_v: f64,
    fps_v: u32,
    silent: bool,
    out_dir: &std::path::Path,
    queue: &Queue,
) -> Result<String, String> {
    let _ = crate::ytdlp::ensure_ytdlp().await;
    let _ = std::fs::create_dir_all(out_dir);
    let send = |t: &str, f: f64| {
        push(queue, Msg::Stage(t.to_string(), f));
    };
    send("Resolving URL...", 0.05);
    let resolved = crate::extractor::resolve_url(url).await;
    let info_url = if resolved != url { resolved.clone() } else { url.to_string() };

    let images_only = mode == 1;
    let slideshow_forced = mode == 2;
    let audio_only = mode == 4;

    if audio_only {
        send("Downloading audio via yt-dlp...", 0.3);
        let title = format!("audio_{}", chrono_stamp());
        let dest = out_dir.join(format!("{}.opus", crate::util::sanitize_filename(&title)));
        return ytdlp_extract(&info_url, &dest, true, queue).await;
    }

    if crate::extractor::is_instagram_url(&info_url) || info_url.contains("tiktok.com") || info_url.contains("tikwm") || info_url.contains("vm.tiktok") || info_url.contains("vt.tiktok") {
        send("Extracting slideshow...", 0.15);
        let info = crate::extractor::extract_info(&info_url).await.map_err(|e| trunc(&e.to_string()))?;
        let fallback = crate::util::extract_id_from_url(&info_url);
        if info.is_video && !slideshow_forced {
            send("Downloading video...", 0.4);
            let out = crate::util::output_path_from_title(&info.title, fallback.as_deref(), &Some(out_dir.to_path_buf()), "mp4");
            crate::downloader::download_direct_video(&info_url, &out).await.map_err(|e| trunc(&e.to_string()))?;
            return Ok(out.display().to_string());
        }
        if info.images.is_empty() {
            return Err("No images found - is this a photo slideshow?".to_string());
        }
        if images_only {
            send(&format!("Downloading {} images...", info.images.len()), 0.4);
            let dir = out_dir.join(crate::util::sanitize_filename(if info.title.is_empty() { "tiktok_images" } else { &info.title }));
            let _ = std::fs::create_dir_all(&dir);
            let tmp = tempfile::tempdir().map_err(|e| trunc(&e.to_string()))?;
            let queue_c = queue.clone();
            let n = info.images.len();
            let cb = std::sync::Arc::new(move |done: u64, _total: u64| {
                push(&queue_c, Msg::Stage(format!("Image {}/{}", done, n), 0.2 + 0.7 * done as f64 / n.max(1) as f64));
            });
            let paths = crate::downloader::download_all_images_with_progress(info.images.clone(), tmp.path(), Some(cb)).await.map_err(|e| trunc(&e.to_string()))?;
            for (i, p) in paths.iter().enumerate() {
                let ext = std::path::Path::new(p).extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                let _ = std::fs::copy(p, dir.join(format!("img_{:03}.{}", i + 1, ext)));
            }
            return Ok(dir.display().to_string());
        }
        send(&format!("Found {} images, downloading...", info.images.len()), 0.3);
        let tmp = tempfile::tempdir().map_err(|e| trunc(&e.to_string()))?;
        let paths = crate::downloader::download_all_images(info.images.clone(), tmp.path()).await.map_err(|e| trunc(&e.to_string()))?;
        let audio = if silent {
            None
        } else if let Some(m) = info.music_url {
            send("Downloading audio...", 0.6);
            crate::downloader::download_audio(&m, tmp.path()).await
        } else {
            None
        };
        send("Rendering swipe video...", 0.75);
        let out = crate::util::output_path_from_title(&info.title, fallback.as_deref(), &Some(out_dir.to_path_buf()), "mp4");
        let queue_c = queue.clone();
        let progress_cb: crate::video::FrameProgressCb = std::sync::Arc::new(move |done: u64, total: u64| {
            let f = if total == 0 { 0.8 } else { 0.75 + 0.2 * done as f64 / total as f64 };
            push(&queue_c, Msg::Stage(format!("Rendering frame {}/{}", done, total), f));
        });
        tokio::task::block_in_place(|| {
            crate::video::create_swipe_video_with_progress(paths, audio, &out, fps_v, trans_v, None, 1080, 1920, Some(progress_cb))
        })
        .map_err(|e| trunc(&e.to_string()))?;
        return Ok(out.display().to_string());
    }

    send("Downloading via yt-dlp...", 0.3);
    let dest = out_dir.join(format!("video_{}.mp4", chrono_stamp()));
    ytdlp_extract(&info_url, &dest, false, queue).await
}

async fn ytdlp_extract(url: &str, dest: &std::path::Path, audio_only: bool, queue: &Queue) -> Result<String, String> {
    let bin = crate::ytdlp::ytdlp_path_async().await;
    let mut cmd = tokio::process::Command::new(&bin);
    if audio_only {
        cmd.args(["-x", "--audio-format", "opus", "--no-playlist", "--no-warnings", "-o"]);
    } else {
        cmd.args(["--no-playlist", "--no-warnings", "-o"]);
    }
    cmd.arg(dest.to_string_lossy().to_string());
    cmd.arg(url);
    push(queue, Msg::Stage("yt-dlp running...".to_string(), 0.5));
    let out = cmd.output().await.map_err(|e| trunc(&e.to_string()))?;
    if !out.status.success() {
        return Err(trunc(&String::from_utf8_lossy(&out.stderr).to_string()));
    }
    Ok(dest.display().to_string())
}

fn trunc(s: &str) -> String {
    let t = s.lines().next().unwrap_or(s).trim();
    t.chars().take(280).collect()
}

fn chrono_stamp() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("{}", d.as_secs() % 1_000_000),
        Err(_) => "dl".to_string(),
    }
}
