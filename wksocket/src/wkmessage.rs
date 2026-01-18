use crate::wksession::{WkSession, PKT_SIZE};
use crate::wkutil::sleep;
use anyhow::{bail, Result};
use bytes::{Buf, BufMut, BytesMut};
use log::trace;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

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
        let (tx, rx) = mpsc::channel();
        let mut buf = BytesMut::with_capacity(PKT_SIZE);
        let mut slots = Vec::<u8>::new();
        let session_closed = Arc::new(AtomicBool::new(false));
        let closed = session_closed.clone();
        thread::spawn(move || loop {
            if let Ok(cmd) = rx.try_recv() {
                match cmd {
                    MessageSND::CloseSession => {
                        let _ = session.close();
                        closed.store(true, Ordering::Relaxed);
                        break;
                    }
                    MessageSND::StartATU => {
                        slots.clear();
                        if let Err(e) = WkSender::encode(&mut buf, PacketKind::StartATU, 0, &slots) {
                            log::error!("encode error: {}", e);
                            continue;
                        }
                        if let Ok(n) = session.send(&buf) {
                            trace!("START ATU {} bytes pkt sent", n);
                        } else {
                            trace!("session closed by peer");
                            let _ = session.close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::SendPacket(tm) => {
                        if let Err(e) = WkSender::encode(&mut buf, PacketKind::KeyerMessage, tm, &slots) {
                            log::error!("encode error: {}", e);
                            continue;
                        }
                        if let Ok(n) = session.send(&buf) {
                            trace!("{} bytes pkt sent at {} edges={}", n, tm, slots.len());
                            buf.clear();
                            slots.clear();
                        } else {
                            trace!("session closed by peer");
                            let _ = session.close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::PosEdge(s) => slots.push(0x80u8 | s),
                    MessageSND::NegEdge(s) => slots.push(s),
                }
            }
            if closed.load(Ordering::Relaxed) {
                break;
            }
            sleep(1);
        });
        Ok(WkSender { session_closed, tx })
    }

    fn encode(buf: &mut BytesMut, cmd: PacketKind, tm: u32, slots: &[u8]) -> Result<()> {
        buf.clear();
        buf.put_u8(cmd as u8);
        buf.put_u32(tm);
        if slots.len() > MAX_SLOTS {
            bail!("Too many slots: {} > {}", slots.len(), MAX_SLOTS);
        }
        buf.put_u8(slots.len() as u8);
        for s in slots.iter() {
            buf.put_u8(*s);
        }
        Ok(())
    }

    pub fn send(&mut self, msg: MessageSND) -> Result<()> {
        if !self.session_closed.load(Ordering::Relaxed) {
            self.tx.send(msg).map_err(|e| anyhow::anyhow!("send error: {}", e))
        } else {
            bail!("session closed by peer")
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
        let (tx, rx) = mpsc::channel::<Vec<MessageRCV>>();

        let session_closed = Arc::new(AtomicBool::new(false));
        let closed = session_closed.clone();
        thread::spawn(move || {
            let mut buf = [0u8; PKT_SIZE];
            loop {
                if let Ok(n) = session.recv(&mut buf) {
                    if n > 0 {
                        let slots = WkReceiver::decode(&buf);
                        if tx.send(slots).is_err() {
                            trace!("receiver dropped, closing session");
                            break;
                        }
                    }
                } else {
                    let slots = vec![MessageRCV::SessionClosed];
                    let _ = tx.send(slots);
                    closed.store(true, Ordering::Relaxed);
                }

                if closed.load(Ordering::Relaxed) {
                    trace!("session closed.");
                    let _ = session.close();
                    break;
                }
                sleep(1);
            }
        });
        Ok(WkReceiver { session_closed, rx })
    }

    pub fn stop(&self) {
        self.session_closed.store(true, Ordering::Relaxed);
    }

    pub fn recv(&self) -> Result<Vec<MessageRCV>> {
        if !self.session_closed.load(Ordering::Relaxed) {
            match self.rx.recv() {
                Ok(s) => Ok(s),
                Err(e) => Err(e.into()),
            }
        } else {
            bail!("session closed")
        }
    }
    pub fn try_recv(&self) -> Result<Vec<MessageRCV>> {
        if !self.session_closed.load(Ordering::Relaxed) {
            match self.rx.try_recv() {
                Ok(s) => Ok(s),
                Err(e) => Err(e.into()),
            }
        } else {
            bail!("session closed")
        }
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
