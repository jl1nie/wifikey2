#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
use wifikey_server::WiFiKeyApp;
// When compiling natively:
fn main() -> eframe::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "trace");
    }
    println!("Log lelvel ={}", std::env::var("RUST_LOG").unwrap());

    egui_logger::init().unwrap(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 200.0])
            .with_min_inner_size([400.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "WiFiKey Sever 2.0",
        native_options,
        Box::new(|cc| Box::new(WiFiKeyApp::new(cc))),
    )
}
