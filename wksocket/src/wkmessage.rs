use crate::wksession::{WkSession, PKT_SIZE};
use anyhow::{anyhow, Result};
use bytes::{Buf, BufMut, BytesMut};
use log::{info, trace};
use std::io::Cursor;
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub const MAX_SLOTS: usize = 128;

pub enum PacketKind {
    KeyerMessage,
    StartATU,
}
#[derive(PartialEq)]
pub enum MessageSND {
    SendPacket(u32),
    PosEdge(u8),
    NegEdge(u8),
    CloseSession,
    StartATU,
}
pub struct WkSender {
    session_closed: Arc<AtomicBool>,
    tx: Sender<MessageSND>,
}

impl WkSender {
    pub fn new(session: Arc<WkSession>) -> Result<Self> {
        let (tx, mut rx) = mpsc::channel(1);
        let mut buf = BytesMut::with_capacity(PKT_SIZE);
        let mut slots = Vec::<u8>::new();
        let session_closed = Arc::new(AtomicBool::new(false));
        let closed = session_closed.clone();
        tokio::spawn(async move {
            loop {
                if let Some(cmd) = rx.recv().await {
                    match cmd {
                        MessageSND::CloseSession => {
                            session.close().await.unwrap();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                        MessageSND::StartATU => {
                            slots.clear();
                            WkSender::encode(&mut buf, PacketKind::StartATU, 0, &slots);
                            if let Ok(n) = session.send(&buf).await {
                                trace!("START ATU {} bytes pkt sent", n);
                            } else {
                                trace!("session closed by peer");
                                session.close().await.unwrap();
                                closed.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                        MessageSND::SendPacket(tm) => {
                            WkSender::encode(&mut buf, PacketKind::KeyerMessage, tm, &slots);
                            if let Ok(n) = session.send(&buf).await {
                                trace!("{} bytes pkt sent at {} edges={}", n, tm, slots.len());
                                buf.clear();
                                slots.clear();
                            } else {
                                trace!("session closed by peer");
                                session.close().await.unwrap();
                                closed.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                        MessageSND::PosEdge(s) => slots.push(0x80u8 | s),
                        MessageSND::NegEdge(s) => slots.push(s),
                    }
                } else {
                    break;
                }
                if closed.load(Ordering::Relaxed) {
                    break;
                }
            }
        });
        Ok(WkSender { session_closed, tx })
    }

    fn encode(buf: &mut BytesMut, cmd: PacketKind, tm: u32, slots: &[u8]) {
        buf.clear();
        buf.put_u8(cmd as u8);
        buf.put_u32(tm);
        if slots.len() > MAX_SLOTS {
            panic! {"Too many slots."}
        }
        buf.put_u8(slots.len() as u8);
        for s in slots.iter() {
            buf.put_u8(*s);
        }
    }

    pub async fn send(&mut self, msg: MessageSND) -> Result<()> {
        if !self.session_closed.load(Ordering::Relaxed) {
            self.tx.send(msg).await?;
            Ok(())
        } else {
            Err(anyhow!("session closed by peer"))
        }
    }
}

#[derive(PartialEq)]
pub enum MessageRCV {
    Sync(u32),
    Keydown(u32),
    Keyup(u32),
    SessionClosed,
    StartATU,
}

pub struct WkReceiver {
    session_closed: Arc<AtomicBool>,
    rx: Receiver<Vec<MessageRCV>>,
}

impl WkReceiver {
    pub fn new(session: Arc<WkSession>) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Vec<MessageRCV>>(1);

        let session_closed = Arc::new(AtomicBool::new(false));
        let closed = session_closed.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; PKT_SIZE];
            loop {
                if let Ok(n) = session.recv(&mut buf).await {
                if let Ok(n) = session.recv(&mut buf) {
                    info!("read from session {}", n);
                    if n > 0 {
                        let slots = WkReceiver::decode(&buf);
                        tx.send(slots).await.unwrap();
                    }
                } else {
                    let slots = vec![MessageRCV::SessionClosed];
                    tx.send(slots).await.unwrap();
                    closed.store(true, Ordering::Relaxed);
                }
                if closed.load(Ordering::Relaxed) {
                    trace!("session closed.");
                    session.close().await.unwrap();
                    break;
                }
            }
        });
        Ok(WkReceiver { session_closed, rx })
    }

    pub fn stop(&self) {
        self.session_closed.store(true, Ordering::Relaxed);
    }

    pub async fn recv(&mut self) -> Result<Vec<MessageRCV>> {
        if !self.session_closed.load(Ordering::Relaxed) {
            if let Some(s) = self.rx.recv().await {
                return Ok(s);
            }
    pub fn recv(&self) -> Result<Vec<MessageRCV>> {
        info!("session recv called");
        //if !self.session_closed.load(Ordering::Relaxed) {
        if let Ok(s) = self.rx.recv_timeout(Duration::from_millis(5)) {
            return Ok(s);
        }
        //}
        Err(anyhow!("session closed"))
    }

    pub fn close(&self) {
        self.session_closed.store(true, Ordering::Relaxed)
    }

    pub fn closed(&self) -> bool {
        self.session_closed.load(Ordering::Relaxed)
    }

    fn decode(buf: &[u8]) -> Vec<MessageRCV> {
        let mut buf = Cursor::new(buf);
        let cmd = buf.get_u8();
        let tm = buf.get_u32();
        let len = buf.get_u8();
        let mut slots = Vec::new();

        if cmd == PacketKind::StartATU as u8 {
            slots.push(MessageRCV::StartATU)
        } else if len == 0 {
            trace!("Sync {}", tm);
            slots.push(MessageRCV::Sync(tm))
        } else {
            trace!("Edges {} {} slots", tm, len);
            for _ in 0..len {
                let d = buf.get_u8();
                let tm = tm + (d & 0x7fu8) as u32;
                let keydown = d & 0x80u8 == 0;
                if keydown {
                    slots.push(MessageRCV::Keydown(tm))
                } else {
                    slots.push(MessageRCV::Keyup(tm))
                }
            }
        }
        slots
    }
}
