pub use self::{
    wkmessage::{MessageRCV, MessageSND, WkReceiver, WkSender, MAX_SLOTS},
    wksession::{challenge, response, WkListener, WkSession, PKT_SIZE},
    wkutil::{sleep, tick_count},
};

mod wkmessage;
mod wksession;
mod wkutil;

/// mDNS service type for WiFiKey2 server discovery
pub const MDNS_SERVICE_TYPE: &str = "_wifikey2._udp.local.";
/// mDNS service domain
pub const MDNS_DOMAIN: &str = "local.";
