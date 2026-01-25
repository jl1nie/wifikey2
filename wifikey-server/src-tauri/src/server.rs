use crate::keyer::RemoteKeyer;
use crate::rigcontrol::RigControl;
use anyhow::Result;
use chrono::{DateTime, Local};
use log::{info, trace};
use mqttstunclient::MQTTStunClient;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::atomic::Ordering;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use wksocket::{challenge, WkListener, WkReceiver};

pub struct WiFiKeyConfig {
    server_name: String,
    server_password: String,
    rigcontrol_port: String,
    keying_port: String,
    use_rts_for_keying: bool,
}

impl WiFiKeyConfig {
    pub fn new(
        server_name: String,
        server_password: String,
        rigcontrol_port: String,
        keying_port: String,
        use_rts_for_keying: bool,
    ) -> Self {
        Self {
            server_name,
            server_password,
            rigcontrol_port,
            keying_port,
            use_rts_for_keying,
        }
    }
}

#[derive(Debug)]
pub struct RemoteStats {
    pub peer_address: Arc<Mutex<Option<String>>>,
    pub session_start: Arc<Mutex<Option<String>>>,
    pub session_active: Arc<AtomicBool>,
    pub auth_ok: Arc<AtomicBool>,
    pub atu_start: Arc<AtomicBool>,
    pub wpm: Arc<AtomicUsize>,
    pub pkt: Arc<AtomicUsize>,
    /// Round-trip time in milliseconds (estimated from sync timing)
    pub rtt_ms: Arc<AtomicUsize>,
}

impl Default for RemoteStats {
    fn default() -> Self {
        Self {
            peer_address: Arc::new(Mutex::new(None)),
            session_start: Arc::new(Mutex::new(None)),
            session_active: Arc::new(AtomicBool::new(false)),
            auth_ok: Arc::new(AtomicBool::new(false)),
            atu_start: Arc::new(AtomicBool::new(false)),
            wpm: Arc::new(AtomicUsize::new(0)),
            pkt: Arc::new(AtomicUsize::new(0)),
            rtt_ms: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl RemoteStats {
    #[allow(dead_code)]
    pub fn new(peer: String) -> Arc<Self> {
        Arc::new(Self {
            peer_address: Arc::new(Mutex::new(Some(peer))),
            ..Default::default()
        })
    }

    #[allow(dead_code)]
    pub fn set_peer(&self, name: &str) {
        let mut peer = self.peer_address.lock().expect("lock failed");
        *peer = Some(name.to_string());
    }

    #[allow(dead_code)]
    pub fn clear_peer(&self) {
        let mut peer = self.peer_address.lock().expect("lock failed");
        *peer = None;
    }

    #[allow(dead_code)]
    pub fn set_session_start(&self, start: &str) {
        let mut sinfo = self.session_start.lock().expect("lock failed");
        *sinfo = Some(start.to_string());
    }

    #[allow(dead_code)]
    pub fn clear_session_start(&self) {
        let mut sinfo = self.session_start.lock().expect("lock failed");
        *sinfo = None;
    }

    #[allow(dead_code)]
    pub fn set_session_active(&self, state: bool) {
        self.session_active.store(state, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn set_auth_ok(&self, state: bool) {
        self.auth_ok.store(state, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn set_atu_start(&self, state: bool) {
        self.atu_start.store(state, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn set_stats(&self, wpm: usize, pkt: usize) {
        self.wpm.store(wpm, Ordering::Relaxed);
        self.pkt.store(pkt, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn set_rtt(&self, rtt_ms: usize) {
        self.rtt_ms.store(rtt_ms, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn get_misc_stats(&self) -> (bool, bool, usize, usize, usize) {
        (
            self.auth_ok.load(Ordering::Relaxed),
            self.atu_start.load(Ordering::Relaxed),
            self.wpm.load(Ordering::Relaxed),
            self.pkt.load(Ordering::Relaxed),
            self.rtt_ms.load(Ordering::Relaxed),
        )
    }

    #[allow(dead_code)]
    pub fn get_session_stats(&self) -> HashMap<String, String> {
        let mut stats = HashMap::new();
        let peer = self.peer_address.lock().unwrap();
        if peer.is_some() {
            stats.insert("peer_address".to_string(), peer.clone().unwrap())
        } else {
            stats.insert("peer_address".to_string(), "".to_string())
        };

        let session = self.session_start.lock().unwrap();
        if session.is_some() {
            stats.insert("session_start".to_string(), session.clone().unwrap())
        } else {
            stats.insert("session_start".to_string(), "".to_string())
        };
        stats
    }
}
#[allow(dead_code)]
pub struct WifiKeyServer {
    remote_stats: Arc<RemoteStats>,
    rigcontrol: Arc<RigControl>,
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Drop for WifiKeyServer {
    fn drop(&mut self) {
        info!("wifikey server dropped stop thread.");
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl WifiKeyServer {
    pub fn new(config: Arc<WiFiKeyConfig>, remote_stats: Arc<RemoteStats>) -> Result<Self> {
        let rigcontrol = RigControl::new(
            &config.rigcontrol_port,
            &config.keying_port,
            config.use_rts_for_keying,
        )?;
        let rigcontrol = Arc::new(rigcontrol);
        let stat = remote_stats.clone();
        let config = config.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let quit_thread = stop.clone();
        let rig = rigcontrol.clone();

        let handle = thread::spawn(move || loop {
            if quit_thread.load(Ordering::Relaxed) {
                break;
            }
            let udp = UdpSocket::bind("0.0.0.0:0").unwrap();

            let mut server = MQTTStunClient::new(
                config.server_name.clone(),
                &config.server_password,
                None,
                None,
            );
            if let Some(client_addr) = server.get_client_addr(&udp) {
                let addr = client_addr.to_string();
                info!("Client address: {}", addr);
            } else if let Ok(addr) = udp.local_addr() {
                info!("Local address: {}", addr);
                stat.set_peer(&addr.to_string());
            } else {
                panic!("Failed to get client address");
            }

            let mut listener = WkListener::bind(udp).unwrap();

            match listener.accept() {
                Ok((session, addr)) => {
                    let local_time: DateTime<Local> = Local::now();
                    info!("{}: Accept new session from {}", local_time, addr);
                    stat.set_peer(&addr.to_string());
                    stat.set_session_start(&local_time.format("%F %T").to_string());
                    let Ok(_magic) = challenge(session.clone(), &config.server_password) else {
                        info!("Auth. failure.");
                        stat.set_auth_ok(false);
                        stat.clear_peer();
                        stat.clear_session_start();
                        let _ = session.close();
                        continue;
                    };
                    info!("Auth. Success.");
                    stat.set_auth_ok(true);
                    let mesg = WkReceiver::new(session).unwrap();
                    stat.set_peer(&addr.to_string());
                    let remote = RemoteKeyer::new(stat.clone(), rig.clone());
                    remote.run(mesg);
                    info!("remote keyer disconnected.");
                    stat.set_auth_ok(false);
                    stat.clear_peer();
                    stat.clear_session_start();
                    stat.set_stats(0, 0);
                }
                Err(e) => {
                    trace!("err = {}", e);
                }
            }
        });
        Ok(Self {
            remote_stats,
            rigcontrol,
            stop,
            handle,
        })
    }

    #[allow(dead_code)]
    pub fn start_atu(&self) {
        self.remote_stats.set_atu_start(true);
        self.rigcontrol.start_atu_with_rigcontrol().unwrap();
        self.remote_stats.set_atu_start(false);
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
