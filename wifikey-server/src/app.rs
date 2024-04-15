use crate::{RemoteStats, WiFiKeyConfig, WifiKeyServer};
/// We derive Deserialize/Serialize so we can persist app state on shutdown.
use config::Config;
use std::{sync::Arc, time::Duration};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct WiFiKeyApp {
    accept_port: String,
    rigcontrol_port: String,
    keying_port: String,
    use_rts_for_keying: bool,
    server_password: String,
    sesami: u64,

    #[serde(skip)]
    remote_stats: Arc<RemoteStats>,
    #[serde(skip)]
    server: Arc<WifiKeyServer>,
}

impl Default for WiFiKeyApp {
    fn default() -> Self {
        let config = Config::builder()
            .add_source(config::File::with_name("cfg.toml"))
            .build()
            .unwrap();

        let accept_port = config.get_string("accept_port").unwrap();
        let rigcontrol_port = config.get_string("rigcontrol_port").unwrap();
        let keying_port = config.get_string("keying_port").unwrap();
        let use_rts_for_keying = config.get_bool("use_rts_for_keying").unwrap();
        let server_password = config.get_string("server_password").unwrap();
        let sesami: u64 = config.get_string("sesami").unwrap().parse().unwrap();

        let wk_config = Arc::new(WiFiKeyConfig::new(
            server_password.clone(),
            sesami,
            accept_port.clone(),
            rigcontrol_port.clone(),
            keying_port.clone(),
            use_rts_for_keying,
        ));

        let remote_stats = Arc::new(RemoteStats::default());

        let server = Arc::new(WifiKeyServer::new(wk_config, remote_stats.clone()).unwrap());

        Self {
            accept_port,
            rigcontrol_port,
            keying_port,
            use_rts_for_keying,
            server_password,
            sesami,
            remote_stats,
            server,
        }
    }
}

impl WiFiKeyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }
        Default::default()
    }
}

impl eframe::App for WiFiKeyApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(500));

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });
        let session_stats = self.remote_stats.get_session_stats();
        let (auth, _atu, wpm, pkt) = self.remote_stats.get_misc_stats();
        let wpm = wpm as f32 / 10.0f32;
        let visual = egui::style::Visuals::default();
        let session_active_color = if auth {
            visual.error_fg_color
        } else {
            visual.strong_text_color()
        };
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(
                egui::RichText::new("WiFiKey2")
                    .heading()
                    .color(session_active_color),
            );
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Session start at: ");
                if let Some(str) = session_stats.get("session_start") {
                    ui.label(str);
                }
            });
            ui.horizontal(|ui| {
                ui.label("From: ");
                if let Some(str) = session_stats.get("peer_address") {
                    ui.label(str);
                }
            });
            ui.separator();

            ui.horizontal(|ui| {
                ui.label(wpm.to_string());
                ui.label(" wpm");
                ui.label(pkt.to_string());
                ui.label(" pkt/s");
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Start ATU").clicked() {
                    self.server.start_atu();
                }
            });
        });
        egui::Window::new("Log").show(ctx, |ui| {
            egui_logger::logger_ui(ui);
        });
    }
}
