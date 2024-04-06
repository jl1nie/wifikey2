use anyhow::Result;
use config::Config;
use log::trace;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use wksocket::{WkAuth, WkListener, WkReceiver};
mod keyer;
use keyer::Morse;

mod rigcontrol;
use rigcontrol::RigControl;

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

    let rigcontrol = RigControl::new(
        &config.get_string("rigcontrol_port").unwrap(),
        &config.get_string("keying_port").unwrap(),
        config.get_bool("use_rts_for_keying").unwrap(),
    )?;

    let rigcontrol = Arc::new(rigcontrol);

    let addr = config
        .get_string("accept_port")
        .unwrap()
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    println!("Listening {}", addr);

    let mut listener = WkListener::bind(addr).unwrap();

    loop {
        match listener.accept() {
            Ok((session, addr)) => {
                println!("Accept new session from {}", addr);
                let Ok(_magic) = WkAuth::challenge(
                    session.clone(),
                    &config.get_string("server_password").unwrap(),
                    config.get_string("sesami").unwrap().parse().unwrap(),
                ) else {
                    println!("Auth. failure.");
                    session.close()?;
                    continue;
                };
                println!("Auth. Success.");
                let mesg = WkReceiver::new(session)?;
                let morse = Morse::new(rigcontrol.clone()).unwrap();
                morse.run(mesg);
                println!("Sesstion closed.");
            }
            Err(e) => {
                trace!("err = {}", e)
            }
        }
    }
}
