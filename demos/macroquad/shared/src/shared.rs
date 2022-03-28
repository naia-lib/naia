use std::time::Duration;

use naia_shared::{CompressionConfig, LinkConditionerConfig, SharedConfig, SocketConfig};

use super::channels::{channels_init, Channels};

pub fn shared_config() -> SharedConfig<Channels> {
    // Set tick rate to ~60 FPS
    let tick_interval = Some(Duration::from_millis(20));

    //let link_condition = None;
    //let link_condition = Some(LinkConditionerConfig::average_condition());
    let link_condition = Some(LinkConditionerConfig {
        incoming_latency: 150,
        incoming_jitter: 1,
        incoming_loss: 0.3,
    });

    //let compression_dictionary = fs::read("dictionary.txt").expect("Error reading
    // compression dictionary");

    return SharedConfig::new(
        SocketConfig::new(link_condition, None),
        channels_init(),
        tick_interval,
        Some(CompressionConfig::new(
            None,
            //Some(CompressionMode::Dictionary(22, compression_dictionary)),
            //Some(CompressionMode::Training(1000)),
            None,
        )),
    );
}
