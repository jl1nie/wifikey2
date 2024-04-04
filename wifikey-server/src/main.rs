use anyhow::Result;
use log::{error, info, trace};
use serialport::SerialPortInfo;
use std::io::{stdin, stdout, Write};
use std::net::ToSocketAddrs;
use wksocket::{WkAuth, WkListener, WkReceiver};

mod keyer;
use keyer::Morse;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    server_password: &'static str,
    #[default("0.0.0.0:8080")]
    accept_port: &'static str,
}

fn main() -> Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "error");
    }
    println!("Log lelvel ={}", std::env::var("RUST_LOG").unwrap());
    env_logger::init();

    let ports = serialport::available_ports().expect("No ports found!");
    let port_name = select_port(&ports)?;
    println!("Set serial port to {}", port_name);

    let addr = CONFIG
        .accept_port
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
                let auth = WkAuth::new(session.clone());
                if auth.challenge(CONFIG.server_password).is_err() {
                    println!("Auth. Failed");
                    session.close();
                    continue;
                }
                println!("Auth.Success");
                let mesg = WkReceiver::new(session)?;
                let morse = Morse::new(port_name).unwrap();
                morse.run(mesg);
                println!("Sesstion timeout.");
            }
            Err(e) => {
                trace!("err = {}", e)
            }
        }
    }
}

fn select_port(ports: &[SerialPortInfo]) -> Result<&str> {
    let mut pstr = String::new();
    let mut pnum: usize = 0;
    for (num, p) in ports.iter().enumerate() {
        println!("{}: {}", num, p.port_name);
    }
    loop {
        print!("Select:");
        stdout().flush().unwrap();
        stdin().read_line(&mut pstr)?;
        pnum = pstr.trim().parse()?;
        if pnum < ports.len() {
            break;
        }
        println!("Invalid Port {}", pnum);
        pstr.clear();
    }
    Ok(&ports[pnum].port_name)
}
