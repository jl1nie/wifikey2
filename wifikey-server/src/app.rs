use crate::{RemoteStatics, WiFiKeyConfig, WifiKeyServer};
/// We derive Deserialize/Serialize so we can persist app state on shutdown.
use config::Config;
use std::{
    fmt::format,
    sync::{atomic::Ordering, Arc},
};

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
    remote_statics: Arc<RemoteStatics>,
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

        let config = Arc::new(WiFiKeyConfig::new(
            server_password.clone(),
            sesami,
            accept_port.clone(),
            rigcontrol_port.clone(),
            keying_port.clone(),
            use_rts_for_keying,
        ));

        let remote_statics = Arc::new(RemoteStatics::default());

        let server = Arc::new(WifiKeyServer::new(config, remote_statics.clone()).unwrap());

        Self {
            accept_port,
            rigcontrol_port,
            keying_port,
            use_rts_for_keying,
            server_password,
            sesami,
            remote_statics,
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
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.add_space(16.0);
                egui::widgets::global_dark_light_mode_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("WiFiKey2");

            ui.horizontal(|ui| {
                ui.label("Peer Address: ");
                let peer = self.remote_statics.peer_address.lock().unwrap();
                ui.label(if peer.is_some() {
                    peer.unwrap().to_string()
                } else {
                    "".to_owned()
                });
            });

            ui.horizontal(|ui| {
                ui.label("Active: ");
                let session = self.remote_statics.session_active.load(Ordering::Relaxed);
                ui.label(session.to_string());
            });
            ui.horizontal(|ui| {
                ui.label("Auth Failure: ");
                let session = self.remote_statics.auth_failure.load(Ordering::Relaxed);
                ui.label(session.to_string());
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("WPM: ");
                let wpm = self.remote_statics.wpm.load(Ordering::Relaxed) as f32 / 10.0;
                ui.label(wpm.to_string());
            });
            ui.horizontal(|ui| {
                ui.label("ATU Active: ");
                let atu = self.remote_statics.atu_active.load(Ordering::Relaxed);
                ui.label(atu.to_string());
            });

            ui.separator();
            if ui.button("Start ATU w/o CAT ctrl").clicked() {
                self.server.start_ATU();
            }
        });
    }
}
