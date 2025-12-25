extern crate log;
use log::debug;

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
    let now = Instant::now();
    if Random::gen_range_f32(0.0, 1.0) <= config.incoming_loss {
        debug!("[LINK_COND] Packet dropped due to loss (loss={})", config.incoming_loss);
        return;
    }
    let mut latency: u32 = config.incoming_latency;
    if config.incoming_jitter > 0 {
        if Random::gen_range_f32(0.0, 1.0) < 0.5 {
            latency += Random::gen_range_u32(0, config.incoming_jitter);
        } else {
            // Ensure we don't underflow - clamp to 0 minimum
            let jitter_amount = Random::gen_range_u32(0, config.incoming_jitter);
            if jitter_amount <= latency {
                latency -= jitter_amount;
            } else {
                latency = 0;
            }
        }
    }
    let mut packet_timestamp = now;
    packet_timestamp.add_millis(latency);
    let delay_ms = latency as u64;
    println!("[LINK_COND] Queuing packet: delay={}ms (latency={}, jitter={}, loss={})", 
           delay_ms, config.incoming_latency, config.incoming_jitter, config.incoming_loss);
    time_queue.add_item(packet_timestamp, packet);
    println!("[LINK_COND] Queue length after add: {}", time_queue.len());
}
