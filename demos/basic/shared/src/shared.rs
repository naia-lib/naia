use std::{net::SocketAddr, time::Duration};

use naia_shared::{LinkConditionerConfig, SharedConfig};

use super::protocol::Protocol;

pub fn get_server_address() -> SocketAddr {
    return "127.0.0.1:14191"
        .parse()
        .expect("could not parse socket address from string");
}

pub fn get_shared_config() -> SharedConfig<Protocol> {
    let tick_interval = Some(Duration::from_millis(50));

    // Simulate network conditions with this configuration property
    let link_condition = Some(LinkConditionerConfig::average_condition());
    return SharedConfig::new(Protocol::load(), tick_interval, link_condition);
}
