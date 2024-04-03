use anyhow::{anyhow, bail, Error, Result};
use log::{error, info, trace};
use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::wkutil::{sleep, tick_count};
use kcp::{Kcp, KcpResult};

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

const MTU_SIZE: usize = 512;

pub enum KcpMode {
    Default,
    Normal,
    Fast,
}

pub struct KcpSocket {
    kcp: Kcp<UDPOutput>,
    last_update: u32,
    closed: bool,
    sent_first: bool,
}

impl KcpSocket {
    pub fn new(mode: KcpMode, socket: Arc<UdpSocket>, peer: SocketAddr) -> Result<Self> {
        socket.set_nonblocking(true).unwrap();
        let output = UDPOutput::new(socket.clone(), peer);
        let conv = 0;
        let mut kcp = Kcp::new(conv, output);

        match mode {
            KcpMode::Default => kcp.set_nodelay(false, 10, 0, false),
            KcpMode::Normal => kcp.set_nodelay(false, 10, 0, true),
            KcpMode::Fast => {
                kcp.set_mtu(MTU_SIZE).unwrap();
                kcp.set_nodelay(true, 10, 1, true);
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
            sent_first: false,
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
        if self.closed {
            info!("connection closed.");
            return Ok(0);
        }

        if self.sent_first && self.kcp.waiting_conv() {
            info!("1st packet sent wait for conv");
        }

        let n = self.kcp.send(buf).unwrap();
        self.sent_first = true;
        self.last_update = tick_count();

        self.kcp.flush();

        Ok(n)
    }

    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.closed {
            bail!("session closed")
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
        self.kcp.flush();
        self.last_update = tick_count();
        Ok(())
    }

    pub fn update(&mut self) -> Result<u32> {
        let current = tick_count();
        self.kcp.update(current);
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
    expire: Duration,
}

impl WkSession {
    fn new(udp: Arc<UdpSocket>, peer: SocketAddr, expire: Duration) -> Arc<WkSession> {
        let kcp = KcpSocket::new(KcpMode::Fast, udp.clone(), peer).unwrap();
        let socket = Arc::new(Mutex::new(kcp));
        let server = socket.clone();
        let session = Arc::new(WkSession { socket, expire });
        {
            thread::spawn(move || loop {
                let mut s = server.lock().unwrap();
                if s.closed() {
                    break;
                }
                let n = s.update().unwrap();
                drop(s);
                sleep(n)
            });
        }
        session
    }

    pub fn connect(peer: SocketAddr) -> Result<WkSession> {
        let udp = match peer.ip() {
            IpAddr::V4(..) => UdpSocket::bind("0.0.0.0:0")?,
            IpAddr::V6(..) => UdpSocket::bind("[::]:0")?,
        };
        let udp = Arc::new(udp);
        let kcpudp = udp.clone();
        let kcp = KcpSocket::new(KcpMode::Fast, kcpudp, peer)?;
        let socket = Arc::new(Mutex::new(kcp));

        let client_socket = socket.clone();
        let client_udp = udp.clone();

        thread::spawn(move || {
            let buf = &mut [0u8; 256];
            loop {
                sleep(1);
                match client_udp.recv_from(buf) {
                    Ok((n, src)) => {
                        if src != peer {
                            continue;
                        }
                        let pkt = &mut buf[..n];
                        if pkt.len() < kcp::KCP_OVERHEAD {
                            error!("packet too short {} bytes rewceived from {}", n, peer);
                            continue;
                        }
                        let mut s = client_socket.lock().unwrap();
                        if s.waiting_conv() {
                            let conv = kcp::get_conv(pkt);
                            kcp::set_conv(pkt, conv);
                        }
                        if s.closed() {
                            break;
                        } else {
                            s.input(pkt);
                        }
                    }
                    Err(_) => {}
                }
            }
        });

        let client_socket = socket.clone();
        thread::spawn(move || loop {
            let mut s = client_socket.lock().unwrap();
            if s.closed() {
                break;
            } else {
                let n = s.update().unwrap();
                drop(s);
                sleep(n)
            }
        });

        Ok(WkSession {
            socket,
            expire: Duration::from_secs(30),
        })
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
                sleep(10);
                continue;
            }
        }
        Err(anyhow!("recv timeout"))
    }

    pub fn close(&self) -> Result<()> {
        let mut socket = self.socket.lock().unwrap();
        socket.close();
        Ok(())
    }

    pub fn closed(&self) -> bool {
        let mut socket = self.socket.lock().unwrap();
        socket.closed()
    }
}

pub struct WkListener {
    rx: mpsc::Receiver<(Arc<WkSession>, SocketAddr)>,
}

impl WkListener {
    pub fn bind(addr: SocketAddr) -> Result<Self> {
        let udp = UdpSocket::bind(addr)?;
        let udp = Arc::new(udp);
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 256];
            let mut sessions: Option<(Arc<WkSession>, SocketAddr)> = None;
            loop {
                sleep(10);
                match udp.recv_from(&mut buf) {
                    Err(err) => continue,
                    Ok((n, peer)) => {
                        let pkt = &mut buf[..n];
                        trace!("received {}bytes from {}", n, peer);

                        if pkt.len() < kcp::KCP_OVERHEAD {
                            error!("packet too short {} bytes rewceived from {}", n, peer);
                            continue;
                        }

                        let mut conv = kcp::get_conv(pkt);
                        if conv == 0 {
                            conv = rand::random();
                            info!("set new conv ={}", conv);
                            kcp::set_conv(pkt, conv);
                        }

                        if let Some((ref session, current_peer)) = sessions {
                            if peer == current_peer {
                                info!("input current session {} bytes", n);
                                session.input(pkt);
                                continue;
                            } else {
                                info!("close current session");
                                session.close();
                            }
                        }
                        let session = WkSession::new(udp.clone(), peer, Duration::from_secs(180));
                        info!("input new session {} bytes", n);
                        session.input(pkt);
                        let client_session = session.clone();
                        tx.send((client_session, peer));
                        sessions = Some((session, peer));
                    }
                }
            }
        });
        Ok(WkListener { rx })
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
