use crate::wkutil::{sleep, tick_count};
use anyhow::{bail, Result};
use bytes::{Buf, BufMut, BytesMut};
use kcp::Kcp;
use log::{info, trace};
use md5::{Digest, Md5};
use rand::random;
use std::io::{self, Cursor, Write};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const MTU_SIZE: usize = 512;
pub const SESSION_TIMEOUT: u64 = 15;
pub const PKT_SIZE: usize = 128;

struct UDPOutput {
    socket: Arc<UdpSocket>,
    peer: SocketAddr,
}

impl UDPOutput {
    fn new(socket: Arc<UdpSocket>, peer: SocketAddr) -> Self {
        UDPOutput { socket, peer }
    }
}

impl Write for UDPOutput {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        match self.socket.send_to(data, self.peer) {
            Ok(n) => {
                trace!("{} byte packet sent", n);
                Ok(n)
            }
            Err(e) => {
                trace!("send packet error:{}", e);
                Ok(0)
            }
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
    FastNB,
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
            KcpMode::FastNB => {
                kcp.set_mtu(MTU_SIZE).unwrap();
                kcp.set_nodelay(true, 10, 1, true);
                kcp.set_maximum_resend_times(10);
                socket.set_nonblocking(true).unwrap();
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
        let n = self.kcp.send(buf)?;
        self.last_update = tick_count();
        self.kcp.flush()?;
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

impl Drop for WkSession {
    fn drop(&mut self) {
        info!("session dropped. stop thread.");
        self.closed.store(true, Ordering::Relaxed);
    }
}

impl WkSession {
    fn new(
        udp: Arc<UdpSocket>,
        peer: SocketAddr,
        mode: KcpMode,
        expire: Duration,
    ) -> Arc<WkSession> {
        let kcp = KcpSocket::new(mode, udp.clone(), peer).unwrap();
        let socket = Arc::new(Mutex::new(kcp));
        let server = socket.clone();
        let expire = expire.as_millis() as u32;
        let closed = AtomicBool::new(false);

        thread::spawn(move || loop {
            let mut s = server.lock().unwrap();
            if s.closed() {
                break;
            }
            match s.update() {
                Ok(n) => {
                    if tick_count() - s.last_update() > expire {
                        s.close();
                        break;
                    }
                    drop(s);
                    sleep(n)
                }
                Err(e) => {
                    info!("kcp update failed. {}", e);
                    sleep(1000);
                }
            }
        });
        Arc::new(WkSession { socket, closed })
    }

    pub fn connect(peer: SocketAddr) -> Result<Arc<WkSession>> {
        let udp = match peer.ip() {
            IpAddr::V4(..) => UdpSocket::bind("0.0.0.0:0")?,
            IpAddr::V6(..) => UdpSocket::bind("[::]:0")?,
        };
        let udp = Arc::new(udp);
        let client_udp = udp.clone();
        let session = WkSession::new(
            udp,
            peer,
            KcpMode::FastNB,
            Duration::from_secs(SESSION_TIMEOUT),
        );
        let client_socket = session.socket.clone();
        let client_session = session.clone();

        let _handle = thread::spawn(move || {
            let buf = &mut [0u8; PKT_SIZE];
            loop {
                if client_session.closed() {
                    break;
                }
                sleep(1);
                if let Ok((n, src)) = client_udp.recv_from(buf) {
                    if src != peer {
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
                    let mut s = client_socket.lock().unwrap();
                    if s.waiting_conv() {
                        let conv = kcp::get_conv(pkt);
                        kcp::set_conv(pkt, conv);
                    }
                    if s.closed() {
                        break;
                    } else if s.input(pkt).is_err() {
                        info!("conv. id inconsistent.");
                    }
                }
            }
        });

        Ok(session)
    }

    pub fn input(&self, buf: &[u8]) -> Result<()> {
        let mut socket = self.socket.lock().unwrap();
        socket.input(buf)
    }

    pub fn send(&self, buf: &[u8]) -> Result<usize> {
        let mut socket = self.socket.lock().unwrap();
        socket.send(buf)
    }

    pub fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        let mut socket = self.socket.lock().unwrap();
        socket.recv(buf)
    }

    pub fn recv_timeout(&self, buf: &mut [u8], timeout: u32) -> Result<usize> {
        let now = tick_count();
        while tick_count() - now < timeout {
            let mut socket = self.socket.lock().unwrap();
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

    pub fn close(&self) -> Result<()> {
        let mut socket = self.socket.lock().unwrap();
        socket.close();
        self.closed.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

pub struct WkListener {
    stop: Arc<AtomicBool>,
    rx: mpsc::Receiver<(Arc<WkSession>, SocketAddr)>,
}

impl Drop for WkListener {
    fn drop(&mut self) {
        info!("litener dropped. stop thread.");
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl WkListener {
    pub fn bind(addr: SocketAddr) -> Result<Self> {
        let udp = UdpSocket::bind(addr)?;
        let udp = Arc::new(udp);
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        {
            let stop = stop.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 256];
                let mut sessions: Option<(Arc<WkSession>, SocketAddr)> = None;
                loop {
                    if stop.load(Ordering::Relaxed) {
                        info!("stop listner thread");
                        break;
                    }
                    match udp.recv_from(&mut buf) {
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
                                        if session.input(pkt).is_ok() {
                                            continue;
                                        }
                                        info!("conv. id inconsistent. close session");
                                        session.close().unwrap();
                                    } else {
                                        trace!("discard packet");
                                        continue;
                                    }
                                }
                                trace!("session closed.");
                            }
                            trace!("accept new session from peer = {} input {} bytes", peer, n);
                            let session = WkSession::new(
                                udp.clone(),
                                peer,
                                KcpMode::Fast,
                                Duration::from_secs(SESSION_TIMEOUT),
                            );
                            session.input(pkt).unwrap();

                            let client_session = session.clone();
                            tx.send((client_session, peer)).unwrap();
                            sessions = Some((session, peer));
                        }
                    }
                }
            });
        }
        Ok(WkListener { stop, rx })
    }

    pub fn accept(&mut self) -> Result<(Arc<WkSession>, SocketAddr)> {
        match self.rx.recv() {
            Ok((s, addr)) => Ok((s, addr)),
            Err(e) => {
                trace!("accept err={}", e);
                Err(e.into())
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

pub fn response(session: Arc<WkSession>, passwd: &str, sesami: u64) -> Result<u32> {
    let mut sendbuf = BytesMut::with_capacity(PKT_SIZE);
    let mut buf = [0u8; PKT_SIZE];

    sendbuf.put_u64(sesami);
    session.send(&sendbuf)?;

    if session.recv_timeout(&mut buf, 1000).is_err() {
        bail!("auth response time out");
    }

    let mut rcvbuf = Cursor::new(buf);
    let salt = rcvbuf.get_u32();
    hashstr(&mut buf, &format!("{}{}", passwd, salt));
    session.send(&buf)?;

    if session.recv_timeout(&mut buf, 1000).is_err() {
        bail!("auth response time out");
    }
    let mut rcvbuf = Cursor::new(buf);
    let res = rcvbuf.get_u32();
    if res == 0 {
        bail!("auth failed")
    } else {
        Ok(res)
    }
}

pub fn challenge(session: Arc<WkSession>, passwd: &str, sesami: u64) -> Result<u32> {
    let mut sendbuf = BytesMut::with_capacity(PKT_SIZE);
    let mut buf = [0u8; PKT_SIZE];

    if session.recv_timeout(&mut buf, 1000).is_err() {
        info!("auth challenge timeout1");
        bail!("auth challenge timeout1");
    }

    let mut rcvbuf = Cursor::new(buf);
    if sesami != rcvbuf.get_u64() {
        info!("auth challenge timeout2");
        bail!("auth challenge timeout2");
    }

    let chl = random();
    sendbuf.put_u32(chl);
    session.send(&sendbuf)?;

    if session.recv_timeout(&mut buf, 1000).is_err() {
        info!("auth challenge timeout3");
        bail!("auth challenge timeout3");
    };

    let response = &buf[..16];
    let mut challenge = [0u8; 16];
    hashstr(&mut challenge, &format!("{}{}", passwd, chl));
    let ok = response.iter().eq(challenge.iter());
    let res = if ok { random::<u32>() + 1u32 } else { 0u32 };
    sendbuf.clear();
    sendbuf.put_u32(res);
    session.send(&sendbuf)?;

    if ok {
        info!("auth challenge success {}", res);
        Ok(res)
    } else {
        info!("auth challenge failed {:?} {:?}", response, challenge);
        bail!("auth challenge failed")
    }
}
