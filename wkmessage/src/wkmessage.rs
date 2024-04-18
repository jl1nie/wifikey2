use anyhow::{bail,Result};
use bytes::{Buf, BufMut, BytesMut};
use core::str;
use log::{info, trace};
use md5::{Digest, Md5};
use rand::random;
use std::io::Cursor;
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{timeout, Duration};
use tokio_kcp::KcpStream;

pub const PKT_SIZE: usize = 256;
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
    pub fn new(mut stream: KcpStream) -> Result<Self> {
        let (tx, mut rx) = mpsc::channel(1);
        let mut buf = BytesMut::with_capacity(PKT_SIZE);
        let mut slots = Vec::<u8>::new();
        let session_closed = Arc::new(AtomicBool::new(false));
        let closed = session_closed.clone();
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    MessageSND::CloseSession => {
                        stream.session().close();
                        closed.store(true, Ordering::Relaxed);
                        break;
                    }
                    MessageSND::StartATU => {
                        slots.clear();
                        WkSender::encode(&mut buf, PacketKind::StartATU, 0, &slots);
                        if let Ok(n) = stream.send(&buf).await {
                            trace!("START ATU {} bytes pkt sent", n);
                        } else {
                            trace!("session closed by peer");
                            stream.session().close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::SendPacket(tm) => {
                        WkSender::encode(&mut buf, PacketKind::KeyerMessage, tm, &slots);
                        if let Ok(n) = stream.send(&buf).await {
                            trace!("{} bytes pkt sent at {} edges={}", n, tm, slots.len());
                            buf.clear();
                            slots.clear();
                        } else {
                            trace!("session closed by peer");
                            stream.session().close();
                            closed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    MessageSND::PosEdge(s) => slots.push(0x80u8 | s),
                    MessageSND::NegEdge(s) => slots.push(s),
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
    pub fn new(mut stream: KcpStream) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Vec<MessageRCV>>(1);

        let session_closed = Arc::new(AtomicBool::new(false));
        let closed = session_closed.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; PKT_SIZE];
            loop {
                if let Ok(n) = stream.recv(&mut buf).await {
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
                    stream.session().close();
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
        }
        bail!("session closed")
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

fn hashstr(buf: &mut [u8], passwd: &str) {
    let mut hasher = Md5::new();
    hasher.update(passwd);
    let result = hasher.finalize();
    let buf = &mut buf[..16];
    buf.copy_from_slice(&result);
}

pub async fn response(stream: &mut KcpStream, passwd: &str, sesami: u64) -> Result<u32> {
    let mut sendbuf = BytesMut::with_capacity(PKT_SIZE);
    let mut buf = [0u8; PKT_SIZE];

    sendbuf.put_u64(sesami);
    stream.send(&sendbuf).await?;

    if let Err(e) = timeout(Duration::from_secs(5), async {
        stream.recv(&mut buf).await.unwrap();
    })
    .await
    .map_err(|_| "auth open timed out")
    {
        info!("{}", e);
        bail!(e)
    }

    let mut rcvbuf = Cursor::new(buf);
    let salt = rcvbuf.get_u32();
    hashstr(&mut buf, &format!("{}{}", passwd, salt));
    stream.send(&buf).await?;

    if let Err(e) = timeout(Duration::from_secs(5), async {
        stream.recv(&mut buf).await.unwrap();
    })
    .await
    .map_err(|_| "auth response time out")
    {
        info!("{}", e);
        bail!(e)
    }

    let mut rcvbuf = Cursor::new(buf);
    let res = rcvbuf.get_u32();
    if res == 0 {
        info!("password error auth. failed.");
        bail!("password error auth. failed.")
    } else {
        Ok(res)
    }
}

pub async fn challenge(stream: &mut KcpStream, passwd: &str, sesami: u64) -> Result<u32> {
    let mut sendbuf = BytesMut::with_capacity(PKT_SIZE);
    let mut buf = [0u8; PKT_SIZE];

    if let Err(e) = timeout(Duration::from_secs(10), async {
        stream.recv(&mut buf).await.unwrap();
    })
    .await
    .map_err(|_| "no open message.")
    {
        info!("{}", e);
        bail!(e)
    }

    let mut rcvbuf = Cursor::new(buf);
    if sesami != rcvbuf.get_u64() {
        trace!("can not open sesami");
        bail!("auth challenge timeout");
    }

    let chl = random();
    sendbuf.put_u32(chl);
    stream.send(&sendbuf).await?;

    if let Err(e) = timeout(Duration::from_secs(10), async {
        stream.recv(&mut buf).await.unwrap();
    })
    .await
    .map_err(|_| "auth. challenge timeout.")
    {
        info!("{}", e);
        bail!(e)
    }

    let response = &buf[..16];
    let mut challenge = [0u8; 16];
    hashstr(&mut challenge, &format!("{}{}", passwd, chl));
    let ok = response.iter().eq(challenge.iter());
    let res = if ok { random::<u32>() + 1u32 } else { 0u32 };

    sendbuf.clear();
    sendbuf.put_u32(res);
    stream.send(&sendbuf).await?;

    if ok {
        info!("challenge successe {}", res);
        Ok(res)
    } else {
        info!("challenge failed {:?} {:?}", response, challenge);
        bail!("auth challenge failed")
    }
}
