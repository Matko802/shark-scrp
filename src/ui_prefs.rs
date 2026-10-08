use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use crate::compress::target;
use crate::presets::AppSettings;

fn section_title(parent: &gtk4::Box, text: &str) {
    let l = gtk4::Label::new(Some(text));
    l.set_xalign(0.0);
    l.add_css_class("title-4");
    parent.append(&l);
}

fn output_label_text(s: &AppSettings) -> String {
    match &s.output_dir {
        Some(d) => format!("Output: {}", d.display()),
        None => "Output: same folder as input".to_string(),
    }
}

pub fn build_settings_page(settings: Rc<RefCell<AppSettings>>) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    section_title(&root, "Output");
    let out_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let out_label = gtk4::Label::new(Some(&output_label_text(&settings.borrow())));
    out_label.set_xalign(0.0);
    out_label.set_hexpand(true);
    out_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    let choose_btn = gtk4::Button::with_label("Choose folder");
    let clear_btn = gtk4::Button::with_label("Use input folder");
    out_row.append(&out_label);
    out_row.append(&choose_btn);
    out_row.append(&clear_btn);
    root.append(&out_row);
    root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    section_title(&root, "Compress defaults");
    let target_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let preset_labels: Vec<&str> = target::TARGET_PRESETS.iter().map(|p| p.0).collect();
    let target_drop = gtk4::DropDown::from_strings(&preset_labels);
    let mb_spin = gtk4::SpinButton::with_range(0.5, 2000.0, 1.0);
    mb_spin.set_value(settings.borrow().target_mb);
    let closest = target::TARGET_PRESETS.iter().position(|p| p.1 > 0.0 && (p.1 - settings.borrow().target_mb).abs() < 0.5).unwrap_or(6) as u32;
    target_drop.set_selected(closest);
    target_row.append(&gtk4::Label::new(Some("Target:")));
    target_row.append(&target_drop);
    target_row.append(&mb_spin);
    target_row.append(&gtk4::Label::new(Some("MB")));
    root.append(&target_row);

    let effort_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let effort_drop = gtk4::DropDown::from_strings(target::Effort::all());
    effort_drop.set_selected(settings.borrow().target_effort);
    effort_row.append(&gtk4::Label::new(Some("Effort:")));
    effort_row.append(&effort_drop);
    root.append(&effort_row);

    let remember_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    remember_row.append(&gtk4::Label::new(Some("Remember compress choices:")));
    let remember_switch = gtk4::Switch::new();
    remember_switch.set_active(settings.borrow().remember_target);
    remember_row.append(&remember_switch);
    root.append(&remember_row);
    root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    section_title(&root, "Download defaults");
    let mode_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let modes = ["Auto", "TikTok images only", "TikTok slideshow video", "Best video (yt-dlp)", "Audio only (yt-dlp)"];
    let mode_drop = gtk4::DropDown::from_strings(&modes);
    mode_drop.set_selected(settings.borrow().download_mode);
    mode_row.append(&gtk4::Label::new(Some("Mode:")));
    mode_row.append(&mode_drop);
    root.append(&mode_row);

    let dl_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let trans_spin = gtk4::SpinButton::with_range(0.2, 1.5, 0.1);
    trans_spin.set_value(settings.borrow().transition_s);
    let fps_spin = gtk4::SpinButton::with_range(15.0, 60.0, 1.0);
    fps_spin.set_value(settings.borrow().fps as f64);
    let no_music = gtk4::CheckButton::with_label("No music");
    no_music.set_active(settings.borrow().no_music);
    dl_row.append(&gtk4::Label::new(Some("Transition s:")));
    dl_row.append(&trans_spin);
    dl_row.append(&gtk4::Label::new(Some("FPS:")));
    dl_row.append(&fps_spin);
    dl_row.append(&no_music);
    root.append(&dl_row);
    root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    section_title(&root, "General");
    let jobs_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    jobs_row.append(&gtk4::Label::new(Some("Parallel jobs:")));
    let jobs_spin = gtk4::SpinButton::with_range(1.0, 8.0, 1.0);
    jobs_spin.set_value(settings.borrow().parallel_jobs as f64);
    jobs_row.append(&jobs_spin);
    root.append(&jobs_row);

    let keep_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    keep_row.append(&gtk4::Label::new(Some("Keep originals:")));
    let keep_switch = gtk4::Switch::new();
    keep_switch.set_active(settings.borrow().keep_originals);
    keep_row.append(&keep_switch);
    root.append(&keep_row);
    root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    section_title(&root, "External tools");
    for s in crate::tools::probe_all() {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let name = gtk4::Label::new(Some(s.id.label()));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        let ver = gtk4::Label::new(Some(&s.version.clone().unwrap_or_else(|| "not found".to_string())));
        ver.add_css_class("dim-label");
        ver.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        let badge = gtk4::Label::new(if s.available() { Some("ready") } else { Some("missing") });
        row.append(&name);
        row.append(&ver);
        row.append(&badge);
        root.append(&row);
    }
    root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let reset_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    let reset_btn = gtk4::Button::new();
    reset_btn.set_label("Reset");
    reset_btn.set_icon_name("view-refresh-symbolic");
    reset_btn.add_css_class("destructive-action");
    reset_btn.set_halign(gtk4::Align::End);
    reset_row.append(&spacer);
    reset_row.append(&reset_btn);
    root.append(&reset_row);

    {
        let sc = settings.clone();
        let lc = out_label.clone();
        choose_btn.connect_clicked(move |_| {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                sc.borrow_mut().output_dir = Some(dir);
                crate::presets::save_settings(&sc.borrow());
                lc.set_text(&output_label_text(&sc.borrow()));
            }
        });
    }
    {
        let sc = settings.clone();
        let lc = out_label.clone();
        clear_btn.connect_clicked(move |_| {
            sc.borrow_mut().output_dir = None;
            crate::presets::save_settings(&sc.borrow());
            lc.set_text(&output_label_text(&sc.borrow()));
        });
    }
    {
        let mc = mb_spin.clone();
        target_drop.connect_selected_notify(move |d| {
            let mb = target::TARGET_PRESETS.get(d.selected() as usize).map(|p| p.1).unwrap_or(-1.0);
            if mb > 0.0 {
                mc.set_value(mb);
            }
        });
    }
    {
        let sc = settings.clone();
        mb_spin.connect_value_changed(move |s| {
            sc.borrow_mut().target_mb = s.value().clamp(0.5, 2000.0);
            crate::presets::save_settings(&sc.borrow());
        });
    }
    {
        let sc = settings.clone();
        effort_drop.connect_selected_notify(move |d| {
            sc.borrow_mut().target_effort = d.selected();
            crate::presets::save_settings(&sc.borrow());
        });
    }
    {
        let sc = settings.clone();
        remember_switch.connect_state_set(move |_, on| {
            sc.borrow_mut().remember_target = on;
            crate::presets::save_settings(&sc.borrow());
            glib::Propagation::Proceed
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
        trans_spin.connect_value_changed(move |s| {
            sc.borrow_mut().transition_s = s.value().clamp(0.2, 1.5);
            crate::presets::save_settings(&sc.borrow());
        });
    }
    {
        let sc = settings.clone();
        fps_spin.connect_value_changed(move |s| {
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
        let sc = settings.clone();
        jobs_spin.connect_value_changed(move |s| {
            sc.borrow_mut().parallel_jobs = (s.value() as usize).clamp(1, 8);
            crate::presets::save_settings(&sc.borrow());
        });
    }
    {
        let sc = settings.clone();
        keep_switch.connect_state_set(move |_, on| {
            sc.borrow_mut().keep_originals = on;
            crate::presets::save_settings(&sc.borrow());
            glib::Propagation::Proceed
        });
    }
    {
        let sc = settings.clone();
        let lc = out_label.clone();
        let td = target_drop.clone();
        let ms = mb_spin.clone();
        let ed = effort_drop.clone();
        let rs = remember_switch.clone();
        let md = mode_drop.clone();
        let ts = trans_spin.clone();
        let fs = fps_spin.clone();
        let nm = no_music.clone();
        let js = jobs_spin.clone();
        let ks = keep_switch.clone();
        let page = root.clone();
        reset_btn.connect_clicked(move |_| {
            let ancestor = page.ancestor(gtk4::Window::static_type());
            let parent: Option<gtk4::Window> = ancestor.and_then(|w| w.downcast::<gtk4::Window>().ok());
            let dlg = adw::MessageDialog::new(parent.as_ref(), Some("Reset settings?"), Some("Are you sure? This restores all defaults."));
            dlg.add_response("cancel", "Cancel");
            dlg.add_response("reset", "Reset");
            dlg.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
            dlg.set_default_response(Some("cancel"));
            dlg.set_close_response("cancel");
            let sc_c = sc.clone();
            let lc_c = lc.clone();
            let td_c = td.clone();
            let ms_c = ms.clone();
            let ed_c = ed.clone();
            let rs_c = rs.clone();
            let md_c = md.clone();
            let ts_c = ts.clone();
            let fs_c = fs.clone();
            let nm_c = nm.clone();
            let js_c = js.clone();
            let ks_c = ks.clone();
            dlg.connect_response(None, move |_, resp| {
                if resp == "reset" {
                    let d = AppSettings::default();
                    *sc_c.borrow_mut() = d.clone();
                    crate::presets::save_settings(&sc_c.borrow());
                    lc_c.set_text(&output_label_text(&d));
                    let closest = target::TARGET_PRESETS.iter().position(|p| p.1 > 0.0 && (p.1 - d.target_mb).abs() < 0.5).unwrap_or(6) as u32;
                    td_c.set_selected(closest);
                    ms_c.set_value(d.target_mb);
                    ed_c.set_selected(d.target_effort);
                    rs_c.set_active(d.remember_target);
                    md_c.set_selected(d.download_mode);
                    ts_c.set_value(d.transition_s);
                    fs_c.set_value(d.fps as f64);
                    nm_c.set_active(d.no_music);
                    js_c.set_value(d.parallel_jobs as f64);
                    ks_c.set_active(d.keep_originals);
                }
            });
            dlg.present();
        });
    }

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&root));
    scroll.into()
}
