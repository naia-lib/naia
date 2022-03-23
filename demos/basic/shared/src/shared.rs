use std::time::Duration;

use naia_shared::{ChannelConfig, LinkConditionerConfig, SharedConfig, SocketConfig, DefaultChannels};

use super::protocol::Protocol;

pub fn shared_config() -> SharedConfig<Protocol, DefaultChannels> {
    let tick_interval = Some(Duration::from_millis(3000));

    // Simulate network conditions with this configuration property
    //let link_condition = Some(LinkConditionerConfig::average_condition());

    let link_condition = Some(LinkConditionerConfig {
        incoming_latency: 50,
        incoming_jitter: 1,
        incoming_loss: 0.5,
    });

    return SharedConfig::new(
        Protocol::load(),
        SocketConfig::new(link_condition, None),
        ChannelConfig::default(),
        tick_interval,
        None);
}
