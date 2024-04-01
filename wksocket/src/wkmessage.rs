use anyhow::{bail, Result};
use bytes::{Buf, BufMut, BytesMut};
use log::{error, info, trace};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use crate::wksession::WkSession;
use crate::wkutil::sleep;

pub const PKT_SIZE: usize = 128;
pub const MAX_SLOTS: usize = 128;

pub enum MessageSND {
    SendPacket(u32),
    PosEdge(u8),
    NegEdge(u8),
    CloseSession,
}

pub struct WkSender {
    session_closed: Arc<AtomicBool>,
    tx: Sender<MessageSND>,
}

impl WkSender {
    pub fn new(session: WkSession) -> Result<Self> {
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
                        break;
                    }
                    MessageSND::SendPacket(tm) => {
                        WkSender::encode(&mut buf, tm, &slots);
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
            sleep(1);
        });

        Ok(WkSender { session_closed, tx })
    }

    fn encode(buf: &mut BytesMut, tm: u32, slots: &[u8]) {
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

pub enum MessageRCV {
    Sync(u32),
    Keydown(u32),
    Keyup(u32),
    SessionClosed,
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
                {
                    if let Ok(n) = session.recv(&mut buf) {
                        if n > 0 {
                            let slots = WkReceiver::decode(&buf);
                            tx.send(slots).unwrap();
                        }
                    } else {
                        let slots = vec![MessageRCV::SessionClosed];
                        tx.send(slots).unwrap();
                        trace!("session closed by peer");
                        closed.store(true, Ordering::Relaxed);
                        break;
                    }

                    if closed.load(Ordering::Relaxed) {
                        log::trace!("session closed by receiver");
                        session.close();
                        break;
                    }
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
            Ok(self.rx.recv().unwrap())
        } else {
            bail!("WkReceiver session closed")
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
        let tm = buf.get_u32();
        let len = buf.get_u8();
        let mut slots = Vec::new();
        if len == 0 {
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
