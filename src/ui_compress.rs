use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::app::{current_profile, new_rows, settings_output_dir};
use crate::compress;
use crate::presets::{AppSettings, CompressProfile};
use crate::queue::{JobKind, JobStatus, SharedStore};
use crate::util;

enum Msg {
    Progress(u64, f64, String),
    Done(u64, Result<compress::CompressResult, String>),
}

type Queue = Arc<Mutex<VecDeque<Msg>>>;

pub fn build_compress_page(
    store: SharedStore,
    profiles: Rc<RefCell<Vec<CompressProfile>>>,
    settings: Rc<RefCell<AppSettings>>,
) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    let hint = gtk4::Label::new(Some("Drop files or add them, pick a preset, hit Compress. JPEG->MozJPEG, PNG->ECT, GIF->Gifsicle, other->ImageMagick, video/audio->FFmpeg."));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    hint.add_css_class("dim-label");
    root.append(&hint);

    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

    let names: Vec<String> = profiles.borrow().iter().map(|p| p.name.clone()).collect();
    let names_ref: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let preset_drop = gtk4::DropDown::from_strings(&names_ref);
    preset_drop.set_selected(0);
    controls.append(&gtk4::Label::new(Some("Preset:")));
    controls.append(&preset_drop);

    let quality = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 40.0, 95.0, 1.0);
    quality.set_value(78.0);
    quality.set_hexpand(true);
    quality.set_tooltip_text(Some("JPEG quality override for this run"));
    controls.append(&gtk4::Label::new(Some("JPEG q:")));
    controls.append(&quality);

    let add_btn = gtk4::Button::with_label("Add files");
    let out_btn = gtk4::Button::with_label("Output folder");
    let go_btn = gtk4::Button::with_label("Compress");
    go_btn.add_css_class("suggested-action");
    controls.append(&add_btn);
    controls.append(&out_btn);
    controls.append(&go_btn);
    root.append(&controls);

    let adv = gtk4::Expander::new(Some("Advanced"));
    let adv_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    let strip_check = gtk4::CheckButton::with_label("Strip metadata");
    strip_check.set_active(true);
    let crf = gtk4::SpinButton::with_range(16.0, 32.0, 1.0);
    crf.set_value(23.0);
    crf.set_tooltip_text(Some("Video CRF (lower = bigger/better)"));
    adv_box.append(&strip_check);
    adv_box.append(&gtk4::Label::new(Some("Video CRF:")));
    adv_box.append(&crf);
    adv.set_child(Some(&adv_box));
    root.append(&adv);

    let out_label = gtk4::Label::new(Some("Output: same folder as input"));
    out_label.set_xalign(0.0);
    out_label.add_css_class("dim-label");
    root.append(&out_label);

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_min_content_height(280);
    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    scroll.set_child(Some(&list));
    root.append(&scroll);

    let bottom = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let clear_btn = gtk4::Button::with_label("Clear finished");
    let total_label = gtk4::Label::new(Some("Idle"));
    total_label.set_hexpand(true);
    total_label.set_xalign(0.0);
    bottom.append(&total_label);
    bottom.append(&clear_btn);
    root.append(&bottom);

    let rows = new_rows();
    let pending: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
    let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));

    {
        let queue_c = queue.clone();
        let rows_c = rows.clone();
        let store_c = store.clone();
        let total_c = total_label.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
            let msgs: Vec<Msg> = {
                let mut q = queue_c.lock().unwrap();
                q.drain(..).collect()
            };
            for msg in msgs {
                match msg {
                    Msg::Progress(id, frac, text) => {
                        store_c.lock().unwrap().update(id, |j| {
                            j.progress = frac;
                            j.message = text.clone();
                            j.status = JobStatus::Running;
                        });
                        if let Some(r) = rows_c.borrow().get(&id) {
                            r.bar.set_fraction(frac);
                            r.status.set_text(&text);
                        }
                    }
                    Msg::Done(id, res) => {
                        match res {
                            Ok(r) => {
                                store_c.lock().unwrap().update(id, |j| {
                                    j.status = JobStatus::Done;
                                    j.progress = 1.0;
                                    j.output = Some(r.output.clone());
                                    j.before = r.before;
                                    j.after = r.after;
                                    j.message = format!("{} -> {} (-{})", util::format_bytes(r.before), util::format_bytes(r.after), util::savings(r.before, r.after));
                                });
                                if let Some(w) = rows_c.borrow().get(&id) {
                                    w.bar.set_fraction(1.0);
                                    let snap = store_c.lock().unwrap().snapshot();
                                    if let Some(j) = snap.iter().find(|j| j.id == id) {
                                        w.status.set_text(&j.message);
                                        w.info.set_text(&format!("{} -> {}", j.label, j.output.as_ref().map(|p| p.display().to_string()).unwrap_or_default()));
                                    }
                                }
                            }
                            Err(e) => {
                                store_c.lock().unwrap().update(id, |j| {
                                    j.status = JobStatus::Failed;
                                    j.message = e.clone();
                                });
                                if let Some(w) = rows_c.borrow().get(&id) {
                                    w.status.set_text(&format!("Failed: {}", e));
                                }
                            }
                        }
                        let snap = store_c.lock().unwrap().snapshot();
                        let done = snap.iter().filter(|j| matches!(j.status, JobStatus::Done)).count();
                        let failed = snap.iter().filter(|j| matches!(j.status, JobStatus::Failed)).count();
                        total_c.set_text(&format!("{} done, {} failed, {} total", done, failed, snap.len()));
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let pending_c = pending.clone();
        let list_c = list.clone();
        let rows_c = rows.clone();
        let store_c = store.clone();
        add_btn.connect_clicked(move |_| {
            if let Some(files) = rfd::FileDialog::new().pick_files() {
                for f in files {
                    let kind = compress::kind_of(&f);
                    if kind == "unknown" {
                        continue;
                    }
                    let id = store_c.lock().unwrap().add(JobKind::Compress, f.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string());
                    store_c.lock().unwrap().update(id, |j| {
                        j.input = Some(f.clone());
                        j.detail = kind.to_string();
                    });
                    pending_c.borrow_mut().push(f.clone());
                    let row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                    row.set_margin_top(6);
                    row.set_margin_bottom(6);
                    row.set_margin_start(6);
                    row.set_margin_end(6);
                    let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                    let name = gtk4::Label::new(Some(&format!("{}  ({})", f.display(), kind)));
                    name.set_hexpand(true);
                    name.set_xalign(0.0);
                    let status = gtk4::Label::new(Some("Queued"));
                    status.add_css_class("dim-label");
                    top.append(&name);
                    top.append(&status);
                    let bar = gtk4::ProgressBar::new();
                    bar.set_show_text(false);
                    row.append(&top);
                    row.append(&bar);
                    list_c.append(&row);
                    rows_c.borrow_mut().insert(id, crate::app::RowRef { bar, status, info: name });
                }
            }
        });
    }

    {
        let settings_c = settings.clone();
        let out_label_c = out_label.clone();
        out_btn.connect_clicked(move |_| {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                settings_c.borrow_mut().output_dir = Some(dir.clone());
                crate::presets::save_settings(&settings_c.borrow());
                out_label_c.set_text(&format!("Output: {}", dir.display()));
            }
        });
    }

    {
        let pending_c = pending.clone();
        let store_c = store.clone();
        let profiles_c = profiles.clone();
        let settings_c = settings.clone();
        let preset_drop_c = preset_drop.clone();
        let quality_c = quality.clone();
        let strip_c = strip_check.clone();
        let crf_c = crf.clone();
        let queue_c = queue.clone();
        go_btn.connect_clicked(move |_| {
            let items: Vec<std::path::PathBuf> = std::mem::take(&mut *pending_c.borrow_mut());
            if items.is_empty() {
                return;
            }
            let preset_name = preset_drop_c.selected_item().and_then(|o| o.downcast::<gtk4::StringObject>().ok()).map(|o| o.string().to_string()).unwrap_or_else(|| "Balanced".to_string());
            let mut profile = current_profile(&profiles_c, &preset_name);
            profile.jpeg_quality = quality_c.value() as u8;
            profile.strip_metadata = strip_c.is_active();
            profile.video_crf = crf_c.value() as u8;
            let out_dir = settings_output_dir(&settings_c);

            let snap = store_c.lock().unwrap().snapshot();
            for path in items {
                let job = snap.iter().find(|j| j.input.as_ref() == Some(&path) && j.status == JobStatus::Queued).map(|j| j.id);
                let Some(id) = job else { continue };
                let queue_cc = queue_c.clone();
                let profile_c = profile.clone();
                let out_dir_c = out_dir.clone();
                let path_c = path.clone();
                let push = |m: Msg| {
                    queue_cc.lock().unwrap().push_back(m);
                };
                push(Msg::Progress(id, 0.02, "Queued...".to_string()));
                std::thread::spawn(move || {
                    queue_cc.lock().unwrap().push_back(Msg::Progress(id, 0.05, "Compressing...".to_string()));
                    let output = compress::default_output_path(&path_c, out_dir_c.as_deref());
                    match compress::compress_one(&path_c, &output, &profile_c) {
                        Ok(r) => {
                            queue_cc.lock().unwrap().push_back(Msg::Done(id, Ok(r)));
                        }
                        Err(e) => {
                            queue_cc.lock().unwrap().push_back(Msg::Done(id, Err(e.to_string().chars().take(220).collect())));
                        }
                    }
                });
            }
        });
    }

    {
        let store_c = store.clone();
        let list_c = list.clone();
        let rows_c = rows.clone();
        clear_btn.connect_clicked(move |_| {
            store_c.lock().unwrap().clear_finished();
            while let Some(row) = list_c.last_child() {
                list_c.remove(&row);
            }
            rows_c.borrow_mut().clear();
            for j in store_c.lock().unwrap().snapshot() {
                let row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let name = gtk4::Label::new(Some(&j.label));
                name.set_hexpand(true);
                name.set_xalign(0.0);
                let status = gtk4::Label::new(Some(&j.message));
                top.append(&name);
                top.append(&status);
                let bar = gtk4::ProgressBar::new();
                bar.set_fraction(j.progress);
                row.append(&top);
                row.append(&bar);
                list_c.append(&row);
                rows_c.borrow_mut().insert(j.id, crate::app::RowRef { bar, status, info: name });
            }
        });
    }

    if let Some(dir) = settings.borrow().output_dir.clone() {
        out_label.set_text(&format!("Output: {}", dir.display()));
    }

    root.into()
}
