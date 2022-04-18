extern crate log;
// use log::info;

use super::{link_conditioner_config::LinkConditionerConfig, time_queue::TimeQueue, Instant};
use crate::Random;

/// Given a config object which describes the network conditions to be
/// simulated, process an incoming packet, adding it to a TimeQueue at the
/// correct timestamp
pub fn process_packet<T: Eq>(
    config: &LinkConditionerConfig,
    time_queue: &mut TimeQueue<T>,
    packet: T,
) {
    if Random::gen_range_f32(0.0, 1.0) <= config.incoming_loss {
        // drop the packet
        //info!("link conditioner: packet lost");
        return;
    }
    let mut latency: u32 = config.incoming_latency;
    if config.incoming_jitter > 0 {
        if Random::gen_bool() {
            latency += Random::gen_range_u32(0, config.incoming_jitter);
        } else {
            latency -= Random::gen_range_u32(0, config.incoming_jitter);
        }
    }
    let mut packet_timestamp = Instant::now();
    packet_timestamp.add_millis(latency);
    time_queue.add_item(packet_timestamp, packet);
}
