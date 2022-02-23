use std::{time::Duration, fs};

use naia_shared::{
    CompressionConfig, CompressionMode, LinkConditionerConfig,
    PingConfig, SharedConfig, SocketConfig,
};

use super::protocol::Protocol;

pub fn shared_config() -> SharedConfig<Protocol> {
    // Set tick rate to ~60 FPS
    let tick_interval = Some(Duration::from_millis(20));

    //let link_condition = None;
    //let link_condition = Some(LinkConditionerConfig::average_condition());
    let link_condition = Some(LinkConditionerConfig {
        incoming_latency: 150,
        incoming_jitter: 1,
        incoming_loss: 0.0,
    });

    //let compression_dictionary = fs::read("dictionary.txt").expect("Error reading compression dictionary");

    return SharedConfig::new(
        Protocol::load(),
        SocketConfig::new(link_condition, None),
        tick_interval,
        Some(PingConfig::default()),
        Some(CompressionConfig::new(
            None,
            //Some(CompressionMode::Dictionary(22, compression_dictionary)),
            //Some(CompressionMode::Training(10)),
            None,
        )),
    );
}
