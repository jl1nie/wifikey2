use anyhow::Result;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use wksocket::{sleep, tick_count};
use wksocket::{MessageSND, WkSender};
use wksocket::{WkAuth, WkSession};

fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "trace");
    env_logger::init();

    let addr = "localhost:8080".to_socket_addrs().unwrap().next().unwrap();
    for _ in 1..3 {
        let session = WkSession::connect(addr).unwrap();
        let session = Arc::new(session);
        /*
        let auth = WkAuth::new(session.clone());
        if auth.response("Hkello").is_err() {
            continue;
        }*/
        let mut sender = WkSender::new(session).unwrap();
        for _ in 1..5 {
            let mut slot = 0;
            for _ in 1..=5 {
                sender.send(MessageSND::NegEdge(slot))?;
                slot += 10;
                sender.send(MessageSND::PosEdge(slot))?;
                slot += 10;
            }
            sender.send(MessageSND::SendPacket(tick_count()))?;
            sleep(500);
        }
        sender.send(MessageSND::CloseSession)?;
    }
    Ok(())
}
