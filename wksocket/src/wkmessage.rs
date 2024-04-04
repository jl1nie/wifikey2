use anyhow::{anyhow, bail, Result};
use bytes::{Buf, BufMut, BytesMut};
use log::{info, trace};
use rand::random;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use crate::tick_count;
use crate::wksession::WkSession;
use crate::wkutil::sleep;
use md5::{Digest, Md5};

pub const PKT_SIZE: usize = 128;
pub const MAX_SLOTS: usize = 128;
pub const SESSION_TIMEOUT: u32 = 180_000;

pub struct WkAuth {
    session: Arc<WkSession>,
}

impl WkAuth {
    pub fn new(session: Arc<WkSession>) -> Self {
        WkAuth { session }
    }

    pub fn response(&self, passwd: &str) -> Result<()> {
        let mut sendbuf = BytesMut::with_capacity(PKT_SIZE);
        let mut buf = [0u8; PKT_SIZE];

        let req = random();
        sendbuf.put_u32(req);
        self.session.send(&sendbuf);

        if self.session.recv_timeout(&mut buf, 5000).is_err() {
            return Err(anyhow!("auth response time out"));
        }
        let mut rcvbuf = Cursor::new(buf);
        let salt = rcvbuf.get_u32();

        let mut hasher = Md5::new();
        let hashstr = format!("{}{}", passwd, salt);
        hasher.update(&hashstr);
        let result = hasher.finalize();
        let mut buf = &mut buf[..16];
        buf.copy_from_slice(&result);
        self.session.send(&buf);

        if self.session.recv_timeout(&mut buf, 5000).is_err() {
            return Err(anyhow!("auth response time out"));
        }
        let mut rcvbuf = Cursor::new(buf);
        let res = rcvbuf.get_u32();
        if req == res {
            Ok(())
        } else {
            Err(anyhow!("auth response failed"))
        }
    }

    pub fn challenge(&self, passwd: &str) -> Result<()> {
        let mut sendbuf = BytesMut::with_capacity(PKT_SIZE);
        let mut buf = [0u8; PKT_SIZE];

        if self.session.recv_timeout(&mut buf, 5000).is_err() {
            return Err(anyhow!("auth challenge time out"));
        }
        let mut rcvbuf = Cursor::new(buf);
        let req = rcvbuf.get_u32();

        let chl = random();
        sendbuf.put_u32(chl);
        self.session.send(&sendbuf);

        if let Ok(n) = self.session.recv_timeout(&mut buf, 5000) {
            let buf = &buf[..n];
            let mut hasher = Md5::new();
            hasher.update(format!("{}{}", passwd, chl));
            let r = hasher.finalize();
            let hash = r.as_slice();
            sendbuf.clear();
            if buf.iter().eq(hash.iter()) {
                sendbuf.put_u32(req);
                self.session.send(&sendbuf);
                info!("req sent {}", req);
                return Ok(());
            } else {
                sendbuf.put_u32(0);
                self.session.send(&sendbuf);
                return Err(anyhow!("auth challenge failed"));
            }
        }
        Err(anyhow!("auth challenge time out"))
    }
}

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
                        session.close();
                        closed.store(true, Ordering::Relaxed);
                        break;
                    }
                    MessageSND::StartATU => {
                        slots.clear();
                        WkSender::encode(&mut buf, PacketKind::StartATU, 0, &slots);
                        if let Ok(n) = session.send(&buf) {
                            trace!("START ATU {} bytes pkt sent", n);
                        } else {
                            trace!("session closed by peer");
                            session.close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::SendPacket(tm) => {
                        WkSender::encode(&mut buf, PacketKind::KeyerMessage, tm, &slots);
                        if let Ok(n) = session.send(&buf) {
                            trace!("{} bytes pkt sent at {} edges={}", n, tm, slots.len());
                            buf.clear();
                            slots.clear();
                        } else {
                            trace!("session closed by peer");
                            session.close();
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

    pub fn send(&mut self, msg: MessageSND) -> Result<()> {
        if !self.session_closed.load(Ordering::Relaxed) {
            self.tx.send(msg);
            Ok(())
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
        let mut last_received = tick_count();
        thread::spawn(move || {
            let mut buf = [0u8; PKT_SIZE];
            loop {
                if let Ok(n) = session.recv(&mut buf) {
                    if n > 0 {
                        let slots = WkReceiver::decode(&buf);
                        tx.send(slots).unwrap();
                        last_received = tick_count();
                    }
                } else {
                    let slots = vec![MessageRCV::SessionClosed];
                    tx.send(slots).unwrap();
                    trace!("session closed by peer");
                    closed.store(true, Ordering::Relaxed);
                }

                if tick_count() - last_received > SESSION_TIMEOUT {
                    trace!("session timeout");
                    closed.store(true, Ordering::Relaxed);
                }

                if closed.load(Ordering::Relaxed) {
                    log::trace!("session closed by receiver");
                    session.close();
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
            if let Ok(s) = self.rx.recv() {
                return Ok(s);
            }
        }
        bail!("WkReceiver session closed")
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
            slots.push(MessageRCV::Sync(tm))
        } else {
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
