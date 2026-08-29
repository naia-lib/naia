#[cfg(debug_assertions)]
use naia_socket_shared::LinkConditionerConfig;
use naia_socket_shared::SocketConfig;

pub const PING_MSG: &str = "PING";
pub const PONG_MSG: &str = "PONG";

pub fn shared_config() -> SocketConfig {
    // The link conditioner simulates latency, jitter and packet loss. That is
    // what you want while developing, and almost never what you want in a
    // shipped build -- so gate it on the build profile rather than remembering
    // to delete it. Naia deliberately leaves the choice to the app (see
    // naia-lib/naia#65); this is just the pattern.
    #[cfg(debug_assertions)]
    let link_condition = Some(LinkConditionerConfig::average_condition());
    #[cfg(not(debug_assertions))]
    let link_condition = None;

    SocketConfig::new(link_condition, None)
}
