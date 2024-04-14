use anyhow::Result;
use config::Config;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
mod keyer;
mod rigcontrol;

mod server;
use server::{RemoteStats, WiFiKeyConfig, WifiKeyServer};

fn main() -> Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "error");
    }
    println!("Log lelvel ={}", std::env::var("RUST_LOG").unwrap());
    env_logger::init();

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

    let _server = Arc::new(WifiKeyServer::new(wk_config, remote_stats.clone()).unwrap());
    loop {
        println!("{:#?}", remote_stats);
        sleep(Duration::from_secs(2))
    }
    #[allow(unreachable_code)]
    Ok(())
}
