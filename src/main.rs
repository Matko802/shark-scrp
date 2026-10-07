mod app;
mod compress;
mod downloader;
mod extractor;
mod presets;
mod queue;
mod tools;
mod ui_compress;
mod ui_download;
mod ui_prefs;
mod util;
mod video;
mod ytdlp;

fn main() {
    gtk4::init().expect("GTK init failed");
    adw::init().expect("libadwaita init failed");
    app::run();
}
