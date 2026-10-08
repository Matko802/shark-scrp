use adw::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::presets::{self};
use crate::queue::{new_store, SharedStore};

pub fn run() {
    let app = adw::Application::new(Some("io.github.matko802.shark-scrp"), Default::default());
    app.connect_activate(build_window);
    app.run();
}

fn build_window(app: &adw::Application) {
    let store: SharedStore = new_store();
    let settings: Rc<RefCell<presets::AppSettings>> = Rc::new(RefCell::new(presets::load_settings()));

    let window = adw::ApplicationWindow::new(app);
    window.set_title(Some("shark-scrp"));
    window.set_default_size(980, 680);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("shark-scrp", "compress - tiktok - yt-dlp")));

    let tools_btn = gtk4::Button::with_label("Tools");
    header.pack_end(&tools_btn);

    let stack = adw::ViewStack::new();
    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    header.set_title_widget(Some(&switcher));

    let compress_page = crate::ui_compress::build_compress_page(store.clone(), settings.clone());
    let download_page = crate::ui_download::build_download_page(settings.clone());
    stack.add_titled_with_icon(&compress_page, Some("compress"), "Compress", "document-save-symbolic");
    stack.add_titled_with_icon(&download_page, Some("download"), "Download", "folder-download-symbolic");

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&stack));

    let status = crate::tools::probe_all();
    let missing: Vec<String> = status.iter().filter(|s| !s.available()).map(|s| s.id.label().to_string()).collect();
    if !missing.is_empty() {
        let banner = adw::Banner::new(&format!("Missing tools: {}", missing.join(", ")));
        banner.set_revealed(true);
        toolbar_view.add_top_bar(&banner);
    }

    {
        let settings_c = settings.clone();
        tools_btn.connect_clicked(move |_| {
            crate::ui_prefs::show_tools_dialog(settings_c.clone());
        });
    }

    window.set_content(Some(&toolbar_view));
    window.present();
}

pub type RowWidgets = Rc<RefCell<HashMap<u64, RowRef>>>;

#[derive(Clone)]
pub struct RowRef {
    pub bar: gtk4::ProgressBar,
    pub status: gtk4::Label,
    pub info: gtk4::Label,
    pub open: gtk4::Button,
}

pub fn new_rows() -> RowWidgets {
    Rc::new(RefCell::new(HashMap::new()))
}

pub fn settings_output_dir(settings: &Rc<RefCell<presets::AppSettings>>) -> Option<std::path::PathBuf> {
    settings.borrow().output_dir.clone()
}

#[allow(dead_code)]
pub fn _shared(_s: &SharedStore) {
    let _ = Arc::new(Mutex::new(0u32));
}
