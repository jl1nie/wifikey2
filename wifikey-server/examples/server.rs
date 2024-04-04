use anyhow::Result;
use log::{info, trace};
use std::net::ToSocketAddrs;
use std::thread;
use wksocket::{sleep, tick_count};
use wksocket::{MessageRCV, WkReceiver};
use wksocket::{WkAuth, WkListener};

fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "trace");
    env_logger::init();

    let addr = "localhost:8080".to_socket_addrs().unwrap().next().unwrap();
    let mut listener = WkListener::bind(addr).unwrap();

    loop {
        match listener.accept() {
            Ok((session, addr)) => {
                println!("Accept new session from {}", addr);

                let auth = WkAuth::new(session.clone());
                if auth.challenge("Hello").is_err() {
                    info!("auth fail");
                    continue;
                }

                let mesg = WkReceiver::new(session)?;
                thread::spawn(move || loop {
                    match mesg.recv() {
                        Ok(s) => println!("{} slots received ", s.len()),
                        Err(e) => {
                            trace!("err={}", e);
                            break;
                        }
                    }
                    sleep(10)
                });
            }
            Err(e) => trace!("err ={}", e),
        }
    }
    Ok(())
}
