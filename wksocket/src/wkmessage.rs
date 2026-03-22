use crate::wksession::{WkSession, PKT_SIZE};
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
    Ping,
    Pong,
    EncoderEvent = 4,
    ButtonEvent = 5,
}
#[derive(PartialEq)]
pub enum MessageSND {
    SendPacket(u32),
    PosEdge(u8),
    NegEdge(u8),
    CloseSession,
    StartATU,
    Ping(u32),
    Pong(u32),
    EncoderEvent { encoder_id: u8, direction: i8, steps: u8 },
    ButtonEvent { button_id: u8, press_ms: u16 },
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
            // ブロッキング recv で最初のメッセージを待ち、残りは try_iter でドレイン
            let first_cmd = match rx.recv() {
                Ok(cmd) => cmd,
                Err(_) => break,
            };
            for cmd in std::iter::once(first_cmd).chain(rx.try_iter()) {
                match cmd {
                    MessageSND::CloseSession => {
                        let _ = session.close();
                        closed.store(true, Ordering::Relaxed);
                        break;
                    }
                    MessageSND::StartATU => {
                        slots.clear();
                        if let Err(e) = WkSender::encode(&mut buf, PacketKind::StartATU, 0, &slots)
                        {
                            log::error!("encode error: {e}");
                            continue;
                        }
                        if let Ok(n) = session.send(&buf) {
                            trace!("START ATU {n} bytes pkt sent");
                        } else {
                            trace!("session closed by peer");
                            let _ = session.close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::SendPacket(tm) => {
                        if let Err(e) =
                            WkSender::encode(&mut buf, PacketKind::KeyerMessage, tm, &slots)
                        {
                            log::error!("encode error: {e}");
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
                    MessageSND::Ping(ts) => {
                        if let Err(e) = WkSender::encode(&mut buf, PacketKind::Ping, ts, &[]) {
                            log::error!("encode error: {e}");
                            continue;
                        }
                        if let Ok(n) = session.send(&buf) {
                            trace!("Ping {n} bytes sent ts={ts}");
                        } else {
                            trace!("session closed by peer");
                            let _ = session.close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::Pong(ts) => {
                        if let Err(e) = WkSender::encode(&mut buf, PacketKind::Pong, ts, &[]) {
                            log::error!("encode error: {e}");
                            continue;
                        }
                        if let Ok(n) = session.send(&buf) {
                            trace!("Pong {n} bytes sent ts={ts}");
                        } else {
                            trace!("session closed by peer");
                            let _ = session.close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::EncoderEvent { encoder_id, direction, steps } => {
                        // tm = [encoder_id(8)] [dir+128(8)] [steps(8)] [0(8)]
                        let dir_byte = (direction as i16 + 128) as u8;
                        let tm = ((encoder_id as u32) << 24)
                            | ((dir_byte as u32) << 16)
                            | ((steps as u32) << 8);
                        if let Err(e) = WkSender::encode(&mut buf, PacketKind::EncoderEvent, tm, &[]) {
                            log::error!("encode error: {e}");
                            continue;
                        }
                        if let Ok(n) = session.send(&buf) {
                            trace!("KCP sent EncoderEvent {n} bytes enc={encoder_id} dir={direction} steps={steps}");
                        } else {
                            trace!("session closed by peer");
                            let _ = session.close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::ButtonEvent { button_id, press_ms } => {
                        // tm = [0(8)] [button_id(8)] [press_ms(16)]
                        let tm = ((button_id as u32) << 16) | (press_ms as u32);
                        if let Err(e) = WkSender::encode(&mut buf, PacketKind::ButtonEvent, tm, &[]) {
                            log::error!("encode error: {e}");
                            continue;
                        }
                        if let Ok(n) = session.send(&buf) {
                            trace!("ButtonEvent {n} bytes btn={button_id} press_ms={press_ms}");
                        } else {
                            trace!("session closed by peer");
                            let _ = session.close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
            if closed.load(Ordering::Relaxed) {
                break;
            }
        });
        Ok(WkSender { session_closed, tx })
    }

    pub(crate) fn encode(buf: &mut BytesMut, cmd: PacketKind, tm: u32, slots: &[u8]) -> Result<()> {
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

    pub fn send(&self, msg: MessageSND) -> Result<()> {
        if !self.session_closed.load(Ordering::Relaxed) {
            self.tx
                .send(msg)
                .map_err(|e| anyhow::anyhow!("send error: {}", e))
        } else {
            bail!("session closed by peer")
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum MessageRCV {
    Sync(u32),
    Keydown(u32),
    Keyup(u32),
    SessionClosed,
    StartATU,
    Ping(u32),
    Pong(u32),
    EncoderEvent { encoder_id: u8, direction: i8, steps: u8 },
    ButtonEvent { button_id: u8, press_ms: u16 },
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
                // データ到着まで condvar でブロック（最大 100ms タイムアウト）
                match session.recv_wait(&mut buf, 100) {
                    Ok(n) if n > 0 => {
                        let slots = WkReceiver::decode(&buf);
                        if tx.send(slots).is_err() {
                            trace!("receiver dropped, closing session");
                            break;
                        }
                    }
                    Ok(_) => {} // timeout / no data
                    Err(_) => {
                        let slots = vec![MessageRCV::SessionClosed];
                        let _ = tx.send(slots);
                        closed.store(true, Ordering::Relaxed);
                    }
                }

                if closed.load(Ordering::Relaxed) {
                    trace!("session closed.");
                    let _ = session.close();
                    break;
                }
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

    pub(crate) fn decode(buf: &[u8]) -> Vec<MessageRCV> {
        let mut buf = Cursor::new(buf);
        let cmd = buf.get_u8();
        let tm = buf.get_u32();
        let len = buf.get_u8();
        let mut slots = Vec::new();

        if cmd == PacketKind::StartATU as u8 {
            slots.push(MessageRCV::StartATU)
        } else if cmd == PacketKind::Ping as u8 {
            slots.push(MessageRCV::Ping(tm))
        } else if cmd == PacketKind::Pong as u8 {
            slots.push(MessageRCV::Pong(tm))
        } else if cmd == PacketKind::EncoderEvent as u8 {
            let encoder_id = (tm >> 24) as u8;
            let dir_byte = (tm >> 16) as u8;
            let direction = (dir_byte as i16 - 128) as i8;
            let steps = (tm >> 8) as u8;
            slots.push(MessageRCV::EncoderEvent { encoder_id, direction, steps })
        } else if cmd == PacketKind::ButtonEvent as u8 {
            let button_id = (tm >> 16) as u8;
            let press_ms = (tm & 0xFFFF) as u16;
            slots.push(MessageRCV::ButtonEvent { button_id, press_ms })
        } else if len == 0 {
            trace!("Sync {tm}");
            slots.push(MessageRCV::Sync(tm))
        } else {
            trace!("Edges {tm} {len} slots");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_sync_packet() {
        let mut buf = BytesMut::with_capacity(128);
        let slots: &[u8] = &[];
        WkSender::encode(&mut buf, PacketKind::KeyerMessage, 1000, slots).unwrap();

        // Verify: cmd(1) + tm(4) + len(1) = 6 bytes
        assert_eq!(buf.len(), 6);
        assert_eq!(buf[0], PacketKind::KeyerMessage as u8);
        // timestamp is big-endian
        assert_eq!(&buf[1..5], &1000u32.to_be_bytes());
        assert_eq!(buf[5], 0); // no slots
    }

    #[test]
    fn test_encode_with_edges() {
        let mut buf = BytesMut::with_capacity(128);
        let slots: &[u8] = &[0x10, 0x90]; // keydown at +16, keyup at +16
        WkSender::encode(&mut buf, PacketKind::KeyerMessage, 1000, slots).unwrap();

        assert_eq!(buf.len(), 8); // 6 + 2 slots
        assert_eq!(buf[5], 2); // 2 slots
        assert_eq!(buf[6], 0x10); // keydown (high bit = 0)
        assert_eq!(buf[7], 0x90); // keyup (high bit = 1)
    }

    #[test]
    fn test_encode_start_atu() {
        let mut buf = BytesMut::with_capacity(128);
        let slots: &[u8] = &[];
        WkSender::encode(&mut buf, PacketKind::StartATU, 0, slots).unwrap();

        assert_eq!(buf[0], PacketKind::StartATU as u8);
    }

    #[test]
    fn test_encode_too_many_slots() {
        let mut buf = BytesMut::with_capacity(256);
        let slots = vec![0u8; MAX_SLOTS + 1];
        let result = WkSender::encode(&mut buf, PacketKind::KeyerMessage, 0, &slots);

        assert!(result.is_err());
    }

    #[test]
    fn test_decode_sync_packet() {
        let mut buf = BytesMut::with_capacity(128);
        WkSender::encode(&mut buf, PacketKind::KeyerMessage, 1000, &[]).unwrap();

        let msgs = WkReceiver::decode(&buf);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], MessageRCV::Sync(1000));
    }

    #[test]
    fn test_decode_keydown_keyup() {
        let mut buf = BytesMut::with_capacity(128);
        // keydown at offset 10, keyup at offset 20
        let slots: &[u8] = &[10, 0x80 | 20];
        WkSender::encode(&mut buf, PacketKind::KeyerMessage, 1000, slots).unwrap();

        let msgs = WkReceiver::decode(&buf);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], MessageRCV::Keydown(1010)); // 1000 + 10
        assert_eq!(msgs[1], MessageRCV::Keyup(1020)); // 1000 + 20
    }

    #[test]
    fn test_decode_start_atu() {
        let mut buf = BytesMut::with_capacity(128);
        WkSender::encode(&mut buf, PacketKind::StartATU, 0, &[]).unwrap();

        let msgs = WkReceiver::decode(&buf);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], MessageRCV::StartATU);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut buf = BytesMut::with_capacity(128);
        let original_tm = 5000u32;
        let slots: &[u8] = &[5, 0x80 | 10, 15, 0x80 | 25];

        WkSender::encode(&mut buf, PacketKind::KeyerMessage, original_tm, slots).unwrap();
        let msgs = WkReceiver::decode(&buf);

        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0], MessageRCV::Keydown(5005));
        assert_eq!(msgs[1], MessageRCV::Keyup(5010));
        assert_eq!(msgs[2], MessageRCV::Keydown(5015));
        assert_eq!(msgs[3], MessageRCV::Keyup(5025));
    }

    #[test]
    fn test_encoder_event_roundtrip() {
        let mut buf = BytesMut::with_capacity(128);
        let dir_byte = (1i16 + 128) as u8; // direction = +1
        let tm = ((2u32) << 24) | ((dir_byte as u32) << 16) | ((5u32) << 8);
        WkSender::encode(&mut buf, PacketKind::EncoderEvent, tm, &[]).unwrap();

        let msgs = WkReceiver::decode(&buf);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], MessageRCV::EncoderEvent { encoder_id: 2, direction: 1, steps: 5 });
    }

    #[test]
    fn test_encoder_event_negative_direction() {
        let mut buf = BytesMut::with_capacity(128);
        let dir_byte = (-1i16 + 128) as u8; // direction = -1 → 127
        let tm = ((0u32) << 24) | ((dir_byte as u32) << 16) | ((3u32) << 8);
        WkSender::encode(&mut buf, PacketKind::EncoderEvent, tm, &[]).unwrap();

        let msgs = WkReceiver::decode(&buf);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], MessageRCV::EncoderEvent { encoder_id: 0, direction: -1, steps: 3 });
    }

    #[test]
    fn test_button_event_roundtrip() {
        let mut buf = BytesMut::with_capacity(128);
        let tm = ((1u32) << 16) | 1500u32; // button_id=1, press_ms=1500
        WkSender::encode(&mut buf, PacketKind::ButtonEvent, tm, &[]).unwrap();

        let msgs = WkReceiver::decode(&buf);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], MessageRCV::ButtonEvent { button_id: 1, press_ms: 1500 });
    }
}
