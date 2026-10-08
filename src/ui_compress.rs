use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::app::{new_rows, settings_output_dir, RowWidgets};
use crate::compress::{self, target};
use crate::presets::AppSettings;
use crate::queue::{JobKind, JobStatus, SharedStore};
use crate::util;

enum Msg {
    Progress(u64, f64, String),
    Done(u64, Result<compress::CompressResult, String>),
    Fetched(Result<std::path::PathBuf, String>),
}

type Queue = Arc<Mutex<VecDeque<Msg>>>;

fn enqueue_file(
    path: std::path::PathBuf,
    store: &SharedStore,
    pending: &Rc<RefCell<Vec<std::path::PathBuf>>>,
    list: &gtk4::ListBox,
    rows: &RowWidgets,
) {
    let kind = compress::kind_of(&path);
    if kind == "unknown" {
        return;
    }
    let id = store.lock().unwrap().add(JobKind::Compress, path.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string());
    store.lock().unwrap().update(id, |j| {
        j.input = Some(path.clone());
        j.detail = kind.to_string();
    });
    pending.borrow_mut().push(path.clone());
    let row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    row.set_margin_start(6);
    row.set_margin_end(6);
    let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let name = gtk4::Label::new(Some(&format!("{}  ({})", path.display(), kind)));
    name.set_hexpand(true);
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    let status = gtk4::Label::new(Some("Queued"));
    status.add_css_class("dim-label");
    let open = gtk4::Button::with_label("Open");
    open.set_visible(false);
    top.append(&name);
    top.append(&status);
    top.append(&open);
    let bar = gtk4::ProgressBar::new();
    bar.set_show_text(false);
    row.append(&top);
    row.append(&bar);
    list.append(&row);
    rows.borrow_mut().insert(id, crate::app::RowRef { bar, status, info: name, open });
}

pub fn build_compress_page(
    store: SharedStore,
    settings: Rc<RefCell<AppSettings>>,
) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    let format_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let format_drop = gtk4::DropDown::from_strings(target::OutFormat::all());
    format_drop.set_selected(settings.borrow().target_format);
    let effort_drop = gtk4::DropDown::from_strings(target::Effort::all());
    effort_drop.set_selected(settings.borrow().target_effort);
    format_row.append(&gtk4::Label::new(Some("Format:")));
    format_row.append(&format_drop);
    format_row.append(&gtk4::Label::new(Some("Effort:")));
    format_row.append(&effort_drop);
    root.append(&format_row);

    let eff_label = gtk4::Label::new(Some(target::OutFormat::from_index(settings.borrow().target_format).efficiency_hint()));
    eff_label.set_xalign(0.0);
    eff_label.add_css_class("dim-label");
    root.append(&eff_label);

    let files_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let add_btn = gtk4::Button::with_label("Add files");
    let url_entry = gtk4::Entry::new();
    url_entry.set_placeholder_text(Some("Paste URL..."));
    url_entry.set_hexpand(true);
    let fetch_btn = gtk4::Button::with_label("Fetch");
    let go_btn = gtk4::Button::with_label("Compress");
    go_btn.add_css_class("suggested-action");
    files_row.append(&add_btn);
    files_row.append(&url_entry);
    files_row.append(&fetch_btn);
    files_row.append(&go_btn);
    root.append(&files_row);

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
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bottom.append(&spacer);
    bottom.append(&clear_btn);
    root.append(&bottom);

    let rows = new_rows();
    let pending: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
    let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));

    {
        let eff_c = eff_label.clone();
        format_drop.connect_selected_notify(move |d| {
            eff_c.set_text(target::OutFormat::from_index(d.selected()).efficiency_hint());
        });
    }

    {
        let queue_c = queue.clone();
        let rows_c = rows.clone();
        let store_c = store.clone();
        let list_c = list.clone();
        let pending_c = pending.clone();
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
                                        w.info.set_text(&j.label);
                                        let out = j.output.clone();
                                        w.open.set_visible(true);
                                        w.open.connect_clicked(move |_| {
                                            if let Some(p) = &out {
                                                let _ = open::that(p);
                                            }
                                        });
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
                    }
                    Msg::Fetched(res) => {
                        match res {
                            Ok(path) => {
                                enqueue_file(path, &store_c, &pending_c, &list_c, &rows_c);
                            }
                            Err(_) => {
                            }
                        }
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
                    enqueue_file(f, &store_c, &pending_c, &list_c, &rows_c);
                }
            }
        });
    }

    {
        let queue_c = queue.clone();
        let settings_c = settings.clone();
        fetch_btn.connect_clicked(move |_| {
            let url = url_entry.text().to_string().trim().to_string();
            if url.is_empty() {
                return;
            }
            url_entry.set_text("");
            let queue_cc = queue_c.clone();
            let out_c = settings_c.borrow().output_dir.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
                let res = match rt {
                    Ok(rt) => {
                        let dir = out_c.unwrap_or_else(std::env::temp_dir).join("shark-scrp-fetch");
                        rt.block_on(crate::fetch::fetch_url_to_file(&url, &dir)).map_err(|e| {
                            e.to_string().lines().next().unwrap_or("fetch failed").chars().take(200).collect::<String>()
                        })
                    }
                    Err(e) => Err(format!("runtime: {}", e)),
                };
                queue_cc.lock().unwrap().push_back(Msg::Fetched(res));
            });
        });
    }

    {
        let settings_c = settings.clone();
        let out_label_c = out_label.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let text = match settings_c.borrow().output_dir.clone() {
                Some(dir) => format!("Output: {}", dir.display()),
                None => "Output: same folder as input (change in Settings)".to_string(),
            };
            if out_label_c.text().as_str() != text {
                out_label_c.set_text(&text);
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let pending_c = pending.clone();
        let store_c = store.clone();
        let settings_c = settings.clone();
        let format_drop_c = format_drop.clone();
        let effort_drop_c = effort_drop.clone();
        let queue_c = queue.clone();
        go_btn.connect_clicked(move |_| {
            let items: Vec<std::path::PathBuf> = std::mem::take(&mut *pending_c.borrow_mut());
            if items.is_empty() {
                return;
            }
            let spec = target::TargetSpec {
                bytes: target::mb_to_bytes(settings_c.borrow().target_mb),
                format: target::OutFormat::from_index(format_drop_c.selected()),
                effort: target::Effort::from_index(effort_drop_c.selected()),
            };
            {
                let mut s = settings_c.borrow_mut();
                s.target_format = format_drop_c.selected();
                s.target_effort = effort_drop_c.selected();
                if s.remember_target {
                    crate::presets::save_settings(&s);
                }
            }
            let out_dir = settings_output_dir(&settings_c);
            let snap = store_c.lock().unwrap().snapshot();
            let mut started = 0;
            for path in items {
                if file_size_gt(&path, target::MAX_INPUT_BYTES) {
                    if let Some(j) = snap.iter().find(|j| j.input.as_ref() == Some(&path) && j.status == JobStatus::Queued).map(|j| j.id) {
                        store_c.lock().unwrap().update(j, |x| {
                            x.status = JobStatus::Failed;
                            x.message = "File exceeds 2 GiB limit".to_string();
                        });
                        queue_c.lock().unwrap().push_back(Msg::Progress(j, 0.0, "File exceeds 2 GiB limit".to_string()));
                    }
                    continue;
                }
                let job = snap.iter().find(|j| j.input.as_ref() == Some(&path) && j.status == JobStatus::Queued).map(|j| j.id);
                let Some(id) = job else { continue };
                let queue_cc = queue_c.clone();
                let spec_c = spec.clone();
                let out_dir_c = out_dir.clone();
                let path_c = path.clone();
                queue_cc.lock().unwrap().push_back(Msg::Progress(id, 0.02, "Queued...".to_string()));
                std::thread::spawn(move || {
                    let ext = if spec_c.format == target::OutFormat::Auto {
                        let e = path_c.extension().and_then(|x| x.to_str()).unwrap_or("bin").to_lowercase();
                        if e == "jpeg" { "jpg".to_string() } else { e }
                    } else {
                        spec_c.format.extension().to_string()
                    };
                    let output = compress::default_output_path_with_ext(&path_c, out_dir_c.as_deref(), &ext);
                    let queue_prog = queue_cc.clone();
                    let cb: target::TargetProgress = Arc::new(move |d: u64, t: u64| {
                        let f = if t == 0 { 0.1 } else { 0.05 + 0.9 * d as f64 / t as f64 };
                        queue_prog.lock().unwrap().push_back(Msg::Progress(id, f, format!("Pass {}/{}", d, t)));
                    });
                    match target::compress_to_target(&path_c, &output, &spec_c, Some(cb)) {
                        Ok(r) => {
                            queue_cc.lock().unwrap().push_back(Msg::Done(id, Ok(r)));
                        }
                        Err(e) => {
                            queue_cc.lock().unwrap().push_back(Msg::Done(id, Err(e.to_string().chars().take(220).collect())));
                        }
                    }
                });
                started += 1;
            }
            let _ = started;
        });
    }

    {
        let store_c = store.clone();
        let list_c = list.clone();
        let rows_c = rows.clone();
        let pending_c = pending.clone();
        clear_btn.connect_clicked(move |_| {
            store_c.lock().unwrap().clear_finished();
            pending_c.borrow_mut().clear();
            while let Some(row) = list_c.last_child() {
                list_c.remove(&row);
            }
            rows_c.borrow_mut().clear();
            for j in store_c.lock().unwrap().snapshot() {
                if let Some(path) = j.input.clone() {
                    pending_c.borrow_mut().push(path);
                }
                let row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let name = gtk4::Label::new(Some(&j.label));
                name.set_hexpand(true);
                name.set_xalign(0.0);
                name.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
                let status = gtk4::Label::new(Some(&j.message));
                status.add_css_class("dim-label");
                let open = gtk4::Button::with_label("Open");
                open.set_visible(false);
                top.append(&name);
                top.append(&status);
                top.append(&open);
                let bar = gtk4::ProgressBar::new();
                bar.set_fraction(j.progress);
                row.append(&top);
                row.append(&bar);
                list_c.append(&row);
                rows_c.borrow_mut().insert(j.id, crate::app::RowRef { bar, status, info: name, open });
            }
        });
    }

    match settings.borrow().output_dir.clone() {
        Some(dir) => out_label.set_text(&format!("Output: {}", dir.display())),
        None => out_label.set_text("Output: same folder as input (change in Settings)"),
    }

    root.into()
}

fn file_size_gt(path: &std::path::Path, limit: u64) -> bool {
    std::fs::metadata(path).map(|m| m.len() > limit).unwrap_or(false)
}
