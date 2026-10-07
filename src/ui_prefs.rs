use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use crate::presets::AppSettings;

pub fn show_tools_dialog(settings: Rc<RefCell<AppSettings>>) {
    let statuses = crate::tools::probe_all();
    let win = gtk4::Window::new();
    win.set_title(Some("Tools & Settings"));
    win.set_default_size(520, 560);

    let header = gtk4::HeaderBar::new();
    win.set_titlebar(Some(&header));

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    let title = gtk4::Label::new(Some("External tools"));
    title.set_xalign(0.0);
    title.add_css_class("title-4");
    root.append(&title);

    let sub = gtk4::Label::new(Some("FFmpeg video/audio - ImageMagick images - Gifsicle GIF - MozJPEG JPEG - ECT PNG - yt-dlp downloads"));
    sub.set_wrap(true);
    sub.set_xalign(0.0);
    sub.add_css_class("dim-label");
    root.append(&sub);

    for s in &statuses {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let name = gtk4::Label::new(Some(s.id.label()));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        let ver = gtk4::Label::new(Some(&s.version.clone().unwrap_or_else(|| "not found".to_string())));
        ver.add_css_class("dim-label");
        let badge = gtk4::Label::new(if s.available() { Some("ready") } else { Some("missing") });
        row.append(&name);
        row.append(&ver);
        row.append(&badge);
        root.append(&row);
        let role = gtk4::Label::new(Some(s.id.role()));
        role.set_xalign(0.0);
        role.add_css_class("dim-label");
        root.append(&role);
        root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    }

    let jobs_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    jobs_row.append(&gtk4::Label::new(Some("Parallel jobs:")));
    let spin = gtk4::SpinButton::with_range(1.0, 8.0, 1.0);
    spin.set_value(settings.borrow().parallel_jobs as f64);
    jobs_row.append(&spin);
    root.append(&jobs_row);

    let keep_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    keep_row.append(&gtk4::Label::new(Some("Keep originals:")));
    let keep = gtk4::Switch::new();
    keep.set_active(settings.borrow().keep_originals);
    keep_row.append(&keep);
    root.append(&keep_row);

    let hint = gtk4::Label::new(Some("Nix: add ffmpeg imagemagick gifsicle mozjpeg efficient-compression-tool yt-dlp to your shell or flake. The app also auto-fetches yt-dlp from GitHub into ~/.cache/shark-scrp."));
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    root.append(&hint);

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&root));
    win.set_child(Some(&scroll));

    {
        let settings_c = settings.clone();
        spin.connect_value_changed(move |s| {
            settings_c.borrow_mut().parallel_jobs = s.value() as usize;
            crate::presets::save_settings(&settings_c.borrow());
        });
    }
    {
        let settings_c = settings.clone();
        keep.connect_state_set(move |_, on| {
            settings_c.borrow_mut().keep_originals = on;
            crate::presets::save_settings(&settings_c.borrow());
            glib::Propagation::Proceed
        });
    }

    win.present();
}
