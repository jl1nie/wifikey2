#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
use wifikey_server::WiFiKeyApp;

fn main() -> eframe::Result<()> {
    /*
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    egui_logger::builder().init().unwrap();
    */
    egui_logger::builder()
        .filter_level(LevelFilter::Info) // ここでデフォルトを Info に設定！
        .init()
        .unwrap();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 200.0])
            .with_min_inner_size([400.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "WiFiKey2",
        native_options,
        Box::new(|cc| Ok(Box::new(WiFiKeyApp::new(cc)))),
    )
}
