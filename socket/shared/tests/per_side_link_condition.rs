//! naia #11: server and client must be able to condition their inbound
//! paths independently. The shared `Protocol::link_condition` is the common
//! default; per-side configs derive from it via
//! `SocketConfig::with_link_condition` without forking shared protocol state.

use naia_socket_shared::{
    link_condition_logic::process_packet, LinkConditionerConfig, SocketConfig, TimeQueue,
};

const PACKETS: u8 = 16;

fn total_loss() -> LinkConditionerConfig {
    LinkConditionerConfig::new(0, 0, 1.0)
}

#[test]
fn per_side_conditioner_configs_are_independent() {
    // One shared default, as `Protocol::link_condition` sets it.
    let shared = SocketConfig::new(Some(LinkConditionerConfig::perfect_condition()), None);

    // Each side derives its own inbound conditioning from the shared base.
    let server_cfg = shared.with_link_condition(Some(total_loss()));
    let client_cfg = shared.with_link_condition(Some(LinkConditionerConfig::perfect_condition()));

    // Deriving never mutates the shared base ...
    assert_eq!(shared.link_condition.as_ref().unwrap().incoming_loss, 0.0);
    // ... and the two sides differ exactly where they should.
    assert_eq!(
        server_cfg.link_condition.as_ref().unwrap().incoming_loss,
        1.0
    );
    assert_eq!(
        client_cfg.link_condition.as_ref().unwrap().incoming_loss,
        0.0
    );
    // Unrelated fields (e.g. the WebRTC session path) survive derivation.
    assert_eq!(
        server_cfg.rtc_endpoint_path, shared.rtc_endpoint_path,
        "derivation must preserve the endpoint path"
    );
    assert_eq!(
        client_cfg.rtc_endpoint_path, shared.rtc_endpoint_path,
        "derivation must preserve the endpoint path"
    );
}

#[test]
fn per_side_conditioner_configs_drive_inbound_behavior() {
    let shared = SocketConfig::new(Some(LinkConditionerConfig::perfect_condition()), None);
    let server_cfg = shared.with_link_condition(Some(total_loss()));
    let client_cfg = shared.with_link_condition(Some(LinkConditionerConfig::perfect_condition()));

    let mut server_queue: TimeQueue<u8> = TimeQueue::new();
    let mut client_queue: TimeQueue<u8> = TimeQueue::new();
    for i in 0..PACKETS {
        process_packet(
            server_cfg.link_condition.as_ref().unwrap(),
            &mut server_queue,
            i,
        );
        process_packet(
            client_cfg.link_condition.as_ref().unwrap(),
            &mut client_queue,
            i,
        );
    }

    // Deterministic, no sleeps: total inbound loss keeps nothing queued,
    // while the clean side queues every packet.
    assert!(
        server_queue.is_empty(),
        "lossy server inbound must drop every packet"
    );
    assert_eq!(
        client_queue.len(),
        PACKETS as usize,
        "clean client inbound must queue every packet"
    );
}
