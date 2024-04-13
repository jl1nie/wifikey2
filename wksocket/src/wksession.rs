use crate::wkutil::{sleep, tick_count};
use anyhow::{anyhow, bail, Result};
use bytes::{Buf, BufMut, BytesMut};
use kcp::Kcp;
use log::{error, info, trace};
use md5::{Digest, Md5};
use rand::random;
use std::io::{self, Cursor, ErrorKind, Write};
use std::net::{IpAddr, SocketAddr};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::Duration;

pub const MTU_SIZE: usize = 512;
pub const SESSION_TIMEOUT: u64 = 30;
pub const PKT_SIZE: usize = 128;

struct UDPOutput {
    socket: Arc<UdpSocket>,
    peer: SocketAddr,
    delay_tx: mpsc::UnboundedSender<Vec<u8>>,
}

impl UDPOutput {
    fn new(socket: Arc<UdpSocket>, peer: SocketAddr) -> Self {
        let (delay_tx, mut delay_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        {
            let socket = socket.clone();
            tokio::spawn(async move {
                while let Some(buf) = delay_rx.recv().await {
                    info!("delay tx {} bytes", buf.len());
                    if let Err(err) = socket.send_to(&buf, peer).await {
                        error!("[SEND] UDP delayed send failed, error: {}", err);
                    }
                }
            });
        }
        UDPOutput {
            socket,
            peer,
            delay_tx,
        }
    }
}

impl Write for UDPOutput {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        match self.socket.try_send_to(data, self.peer) {
            Ok(n) => {
                info!("{} byte packet sent", n);
                Ok(n)
            }
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
                info!(
                    "[SEND] UDP send EAGAIN, packet.size: {} bytes, delayed send",
                    data.len()
                );

                self.delay_tx
                    .send(data.to_owned())
                    .expect("channel closed unexpectly");

                Ok(data.len())
            }
            Err(e) => Err(e),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[allow(dead_code)]
pub enum KcpMode {
    Default,
    Normal,
    Fast,
}

pub struct KcpSocket {
    kcp: Kcp<UDPOutput>,
    last_update: u32,
    closed: bool,
}

impl KcpSocket {
    pub fn new(mode: KcpMode, socket: Arc<UdpSocket>, peer: SocketAddr) -> Result<Self> {
        let output = UDPOutput::new(socket.clone(), peer);
        let conv = 0;
        let mut kcp = Kcp::new(conv, output);

        match mode {
            KcpMode::Default => kcp.set_nodelay(false, 10, 0, false),
            KcpMode::Normal => kcp.set_nodelay(false, 10, 0, true),
            KcpMode::Fast => {
                kcp.set_mtu(MTU_SIZE).unwrap();
                kcp.set_nodelay(true, 10, 1, true);
                kcp.set_maximum_resend_times(10);
            }
        }

        if conv == 0 {
            kcp.input_conv();
        }

        let last_update = tick_count();
        kcp.update(last_update)?;

        Ok(Self {
            kcp,
            last_update,
            closed: false,
        })
    }

    pub fn input(&mut self, buf: &[u8]) -> Result<()> {
        match self.kcp.input(buf) {
            Ok(_) => {}
            Err(err) => return Err(err.into()),
        }
        self.last_update = tick_count();
        self.kcp.flush_ack()?;
        Ok(())
    }

    pub fn send(&mut self, buf: &[u8]) -> Result<usize> {
        if self.closed || self.kcp.is_dead_link() {
            self.closed = true;
            bail!("connection closed.");
        }
        let n = self.kcp.send(buf).unwrap();
        self.kcp.flush()?;

        let wts = self.kcp.wait_snd();
        trace!("sent {} bytes", n);
        trace!("{} packets are waiting to be sent", wts);
        if wts > 100 {
            bail!("link is dead???")
        }
        self.last_update = tick_count();
        self.kcp.flush();
        Ok(n)
    }

    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.closed {
            bail!("connection closed.");
        }
        match self.kcp.recv(buf) {
            Ok(n) => {
                self.last_update = tick_count();
                Ok(n)
            }
            Err(kcp::Error::RecvQueueEmpty) => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    #[allow(dead_code)]
    pub fn flush(&mut self) -> Result<()> {
        self.kcp.flush()?;
        self.last_update = tick_count();
        Ok(())
    }

    pub fn update(&mut self) -> Result<u32> {
        let current = tick_count();
        self.kcp.update(current)?;
        Ok(self.kcp.check(current))
    }

    #[allow(dead_code)]
    pub fn conv(&mut self) -> u32 {
        self.kcp.conv()
    }

    #[allow(dead_code)]
    pub fn waiting_conv(&mut self) -> bool {
        self.kcp.waiting_conv()
    }

    #[allow(dead_code)]
    pub fn close(&mut self) {
        self.closed = true;
    }

    #[allow(dead_code)]
    pub fn closed(&mut self) -> bool {
        self.closed
    }

    #[allow(dead_code)]
    pub fn last_update(&mut self) -> u32 {
        self.last_update
    }
}

pub struct WkSession {
    socket: Arc<Mutex<KcpSocket>>,
    closed: AtomicBool,
}

impl WkSession {
    async fn new(udp: Arc<UdpSocket>, peer: SocketAddr, expire: Duration) -> Arc<WkSession> {
        let kcp = KcpSocket::new(KcpMode::Fast, udp.clone(), peer).unwrap();
        let socket = Arc::new(Mutex::new(kcp));
        let server = socket.clone();
        let expire = expire.as_millis() as u32;
        let closed = AtomicBool::new(false);

        tokio::spawn(async move {
            loop {
                let mut s = server.lock().await;
                if s.closed() {
                    break;
                }
                let n = s.update().unwrap();
                if tick_count() - s.last_update() > expire {
                    s.close();
                    break;
                }
                drop(s);
                sleep(n);
            }
        });
        Arc::new(WkSession { socket, closed })
    }

    pub async fn connect(peer: SocketAddr) -> Result<Arc<WkSession>> {
        let udp = match peer.ip() {
            IpAddr::V4(..) => UdpSocket::bind("0.0.0.0:0").await?,
            IpAddr::V6(..) => UdpSocket::bind("[::]:0").await?,
        };
        let udp = Arc::new(udp);
        let client_udp = udp.clone();
        let session = WkSession::new(udp, peer, Duration::from_secs(SESSION_TIMEOUT)).await;
        let client_socket = session.socket.clone();
        let client_session = session.clone();

        let _handle = tokio::spawn(async move {
            let buf = &mut [0u8; PKT_SIZE];
            loop {
                if client_session.closed() {
                    break;
                }
                if let Ok((n, src)) = client_udp.recv_from(buf).await {
                    if src != peer {
                        info!("src != peer");
                        continue;
                    }

                    let pkt = &mut buf[..n];
                    if pkt.len() < kcp::KCP_OVERHEAD {
                        info!(
                            "connect: packet too short {} bytes received from {}",
                            n, peer
                        );
                        continue;
                    }
                    let mut s = client_socket.lock().await;
                    if s.waiting_conv() {
                        let conv = kcp::get_conv(pkt);
                        kcp::set_conv(pkt, conv);
                    }
                    if s.closed() {
                        break;
                    } else {
                        info!("input {} bytes from {}", n, src);
                        s.input(pkt).unwrap();
                    }
                }
            }
        });

        Ok(session)
    }

    pub async fn input(&self, buf: &[u8]) -> Result<()> {
        let mut socket = self.socket.lock().await;
        socket.input(buf)
    }

    pub async fn send(&self, buf: &[u8]) -> Result<usize> {
        let mut socket = self.socket.lock().await;
        socket.send(buf)
    }

    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        let mut socket = self.socket.lock().await;
        socket.recv(buf)
    }

    pub async fn recv_timeout(&self, buf: &mut [u8], timeout: u32) -> Result<usize> {
        let now = tick_count();
        while tick_count() - now < timeout {
            let mut socket = self.socket.lock().await;
            if let Ok(n) = socket.recv(buf) {
                if n > 0 {
                    return Ok(n);
                }
                drop(socket);
                sleep(1);
                continue;
            }
        }
        bail!("recv timeout")
    }

    pub async fn close(&self) -> Result<()> {
        let mut socket = self.socket.lock().await;
        socket.close();
        self.closed.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

pub struct WkListener {
    rx: mpsc::Receiver<(Arc<WkSession>, SocketAddr)>,
}

impl WkListener {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        let udp = UdpSocket::bind(addr).await?;
        let udp = Arc::new(udp);
        let (tx, rx) = mpsc::channel(1);

        tokio::spawn(async move {
            let mut buf = [0u8; 256];
            let mut sessions: Option<(Arc<WkSession>, SocketAddr)> = None;
            loop {
                match udp.recv_from(&mut buf).await {
                    Err(_) => continue,
                    Ok((n, peer)) => {
                        let pkt = &mut buf[..n];

                        trace!("received {}bytes from {}", n, peer);

                        if pkt.len() < kcp::KCP_OVERHEAD {
                            info!(
                                "listen: packet too short {} bytes received from {}",
                                n, peer
                            );
                            continue;
                        }

                        let mut conv = kcp::get_conv(pkt);
                        if conv == 0 {
                            conv = rand::random();
                            trace!("set new conv ={}", conv);
                            kcp::set_conv(pkt, conv);
                        }

                        if let Some((ref session, current_peer)) = sessions {
                            if !session.closed() {
                                if peer == current_peer {
                                    trace!("input current session {} bytes", n);
                                    session.input(pkt).await.unwrap();
                                    continue;
                                } else {
                                    trace!("discard packet");
                                    continue;
                                }
                            }
                            trace!("session cloed.");
                        }
                        trace!("accept new session from peer = {} input {} bytes", peer, n);
                        let session =
                            WkSession::new(udp.clone(), peer, Duration::from_secs(SESSION_TIMEOUT))
                                .await;
                        session.input(pkt).await.unwrap();

                        let client_session = session.clone();
                        tx.send((client_session, peer)).await.unwrap();

                        sessions = Some((session, peer));
                    }
                }
            }
        });
        Ok(WkListener { rx })
    }

    pub async fn accept(&mut self) -> Result<(Arc<WkSession>, SocketAddr)> {
        match self.rx.recv().await {
            Some((s, addr)) => Ok((s, addr)),
            None => {
                trace!("listener connection closed");
                bail!("listner connection closed")
            }
        }
    }
}

fn hashstr(buf: &mut [u8], passwd: &str) {
    let mut hasher = Md5::new();
    hasher.update(passwd);
    let result = hasher.finalize();
    let buf = &mut buf[..16];
    buf.copy_from_slice(&result);
}

pub async fn response(session: Arc<WkSession>, passwd: &str, sesami: u64) -> Result<u32> {
    let mut sendbuf = BytesMut::with_capacity(PKT_SIZE);
    let mut buf = [0u8; PKT_SIZE];

    sendbuf.put_u64(sesami);
    session.send(&sendbuf).await?;

    if session.recv_timeout(&mut buf, 1000).await.is_err() {
        info!("auth1 response timeout");
        bail!("auth1 response timeout");
    }

    let mut rcvbuf = Cursor::new(buf);
    let salt = rcvbuf.get_u32();
    hashstr(&mut buf, &format!("{}{}", passwd, salt));
    session.send(&buf).await?;

    if session.recv_timeout(&mut buf, 1000).await.is_err() {
        info!("auth2 response timeout");
        bail!("auth2 response timeout");
    }
    let mut rcvbuf = Cursor::new(buf);
    let res = rcvbuf.get_u32();
    if res == 0 {
        Err(anyhow!("auth failed"))
    } else {
        Ok(res)
    }
}

pub async fn challenge(session: Arc<WkSession>, passwd: &str, sesami: u64) -> Result<u32> {
    let mut sendbuf = BytesMut::with_capacity(PKT_SIZE);
    let mut buf = [0u8; PKT_SIZE];

    if session.recv_timeout(&mut buf, 6000).await.is_err() {
        trace!("auth challenge timeout");
        bail!("auth challenge timeout");
    }

    let mut rcvbuf = Cursor::new(buf);
    if sesami != rcvbuf.get_u64() {
        trace!("cannot open sesami");
        bail!("auth challenge timeout");
    }

    let chl = random();
    sendbuf.put_u32(chl);
    session.send(&sendbuf).await.unwrap();

    if session.recv_timeout(&mut buf, 1000).await.is_err() {
        info!("challenge response timeout");
        bail!("auth challenge time out");
    };

    let response = &buf[..16];
    let mut challenge = [0u8; 16];
    hashstr(&mut challenge, &format!("{}{}", passwd, chl));
    let ok = response.iter().eq(challenge.iter());
    let res = if ok { random::<u32>() + 1u32 } else { 0u32 };
    sendbuf.clear();
    sendbuf.put_u32(res);
    session.send(&sendbuf).await.unwrap();

    if ok {
        info!("challenge successe {}", res);
        Ok(res)
    } else {
        info!("challenge fail {:?} {:?}", response, challenge);
        bail!("auth challenge failed")
    }
}
