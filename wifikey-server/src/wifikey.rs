use crate::keyer::RemoteKeyer;
use crate::rigcontrol::{self, RigControl};
use anyhow::Result;
use log::info;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use wksocket::{WkAuth, WkListener, WkReceiver};

pub struct RemoteStatics {
    pub peer_address: Arc<Mutex<Option<SocketAddr>>>,
    pub session_active: Arc<AtomicBool>,
    pub auth_failure: Arc<AtomicBool>,
    pub atu_active: Arc<AtomicBool>,
    pub wpm: Arc<AtomicUsize>,
}

impl Default for RemoteStatics {
    fn default() -> Self {
        Self {
            peer_address: Arc::new(Mutex::new(None)),
            session_active: Arc::new(AtomicBool::new(false)),
            auth_failure: Arc::new(AtomicBool::new(false)),
            atu_active: Arc::new(AtomicBool::new(false)),
            wpm: Arc::new(AtomicUsize::new(0)),
        }
    }
}
pub struct WiFiKeyConfig {
    server_password: String,
    sesami: u64,
    accept_port: String,
    rigcontrol_port: String,
    keying_port: String,
    use_rts_for_keying: bool,
}

impl WiFiKeyConfig {
    pub fn new(
        server_password: String,
        sesami: u64,
        accept_port: String,
        rigcontrol_port: String,
        keying_port: String,
        use_rts_for_keying: bool,
    ) -> Self {
        Self {
            server_password,
            sesami,
            accept_port,
            rigcontrol_port,
            keying_port,
            use_rts_for_keying,
        }
    }
}

pub struct WifiKeyServer {
    remote_statics: Arc<RemoteStatics>,
    rigcontrol: Arc<RigControl>,
    done: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl WifiKeyServer {
    pub fn new(config: Arc<WiFiKeyConfig>, remote_statics: Arc<RemoteStatics>) -> Result<Self> {
        let rigcontrol = RigControl::new(
            &config.rigcontrol_port,
            &config.keying_port,
            config.use_rts_for_keying,
        )?;
        let rigcontrol = Arc::new(rigcontrol);
        let addr = config
            .accept_port
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();

        info!("Listening {}", addr);

        let mut listener = WkListener::bind(addr).unwrap();
        let rstat = remote_statics.clone();
        let config = config.clone();
        let done = Arc::new(AtomicBool::new(false));
        let quit_thread = done.clone();
        let rig = rigcontrol.clone();

        let handle = thread::spawn(move || loop {
            if quit_thread.load(Ordering::Relaxed) {
                break;
            }
            match listener.accept() {
                Ok((session, addr)) => {
                    info!("Accept new session from {}", addr);
                    //rstat.session_active.store(true, Ordering::Relaxed);
                    let Ok(_magic) =
                        WkAuth::challenge(session.clone(), &config.server_password, config.sesami)
                    else {
                        info!("Auth. failure.");
                        //rstat.auth_failure.store(true, Ordering::Relaxed);
                        session.close();
                        //rstat.session_active.store(false, Ordering::Relaxed);
                        continue;
                    };
                    info!("Auth. Success.");
                    {
                        //let mut peer = rstat.peer_address.lock().unwrap();
                        //*peer = Some(addr);
                        //rstat.auth_failure.store(false, Ordering::Relaxed);
                    }
                    let mesg = WkReceiver::new(session).unwrap();
                    let remote = RemoteKeyer::new(rstat.clone(), rig.clone());
                    remote.run(mesg);
                    info!("spawn remote keyer done.");
                    {
                        //    let mut peer = rstat.peer_address.lock().unwrap();
                        //   *peer = None;
                        //   rstat.session_active.store(false, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    info!("Listener Accept Error {}", e)
                }
            }
        });
        Ok(Self {
            remote_statics,
            rigcontrol,
            done,
            handle,
        })
    }

    pub fn start_ATU(&self) {
        self.rigcontrol.start_atu();
    }

    pub fn stop_thread(&mut self) {
        self.done.store(true, Ordering::Relaxed);
    }
}
