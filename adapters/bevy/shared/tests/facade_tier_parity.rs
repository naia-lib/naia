//! Facade tier parity (Rule C retirement, Usher seq2618).
//!
//! Every symbol the plain tier (`naia_shared`) publishes that a bevy-tier
//! consumer needs must also resolve through the bevy facade
//! (`naia_bevy_shared`). Twice now a name has reached one tier and not the
//! next, found only when a downstream crate tripped over it. Before the fix
//! this file does not compile: `wire_schema_custom_leaf` resolves from
//! neither tier's list and `BitCounter` resolves only from the plain tier.
//! After the fix both paths below resolve to the same items.

use naia_bevy_shared::{wire_schema_custom_leaf as bevy_custom_leaf, BitCounter as BevyBitCounter};
use naia_shared::{wire_schema_custom_leaf as shared_custom_leaf, BitCounter as SharedBitCounter};

#[test]
fn facade_custom_leaf_writer_matches_plain_tier() {
    let mut via_bevy = Vec::new();
    bevy_custom_leaf(&mut via_bevy, "probe.Leaf");
    let mut via_plain = Vec::new();
    shared_custom_leaf(&mut via_plain, "probe.Leaf");
    assert_eq!(via_bevy, via_plain);
}

#[test]
fn facade_bit_counter_is_plain_tier_type() {
    fn takes_bevy_counter(_: BevyBitCounter) {}
    takes_bevy_counter(SharedBitCounter::new(0, 0, u32::MAX));
}

// `RequestOrResponse` (Usher seq2644/2674): the bevy facade must publish the
// plain tier's built-in request/response envelope TYPE so Gerry's
// `session_naia_proto` net-ID tail falsifier can pin it at position 1. Before
// the fix this file does not compile (`unresolved import
// naia_bevy_shared::RequestOrResponse`). The fn-pointer coercion below proves
// both paths name the identical type without needing to construct it (its
// fields are private).
use naia_bevy_shared::RequestOrResponse as BevyRequestOrResponse;
use naia_shared::RequestOrResponse as SharedRequestOrResponse;

#[test]
fn facade_request_or_response_is_plain_tier_type() {
    fn takes_bevy(_: BevyRequestOrResponse) {}
    let _: fn(SharedRequestOrResponse) = takes_bevy;
}

// Envelope-fit proof names (Gerry 72078, Usher 72001): the bevy facade must
// publish the fragmentation limit and the request/response key types so
// cyberlith's envelope-inclusive fit proof can build the real envelope with
// maximum id/nonce against the real limit. Before the fix this file does not
// compile (unresolved imports from naia_bevy_shared).
use naia_bevy_shared::{
    ConnectionRequestNonce as BevyNonce, LocalRequestId as BevyRequestId,
    LocalResponseId as BevyResponseId, FRAGMENTATION_LIMIT_BITS as BevyLimitBits,
    FRAGMENTATION_LIMIT_BYTES as BevyLimitBytes,
};
use naia_shared::{
    ConnectionRequestNonce as SharedNonce, LocalRequestId as SharedRequestId,
    LocalResponseId as SharedResponseId, FRAGMENTATION_LIMIT_BITS as SharedLimitBits,
    FRAGMENTATION_LIMIT_BYTES as SharedLimitBytes,
};

#[test]
fn facade_envelope_fit_names_match_plain_tier() {
    assert_eq!(BevyLimitBits, SharedLimitBits);
    assert_eq!(BevyLimitBytes, SharedLimitBytes);
    assert_eq!(BevyLimitBits, (BevyLimitBytes as u32) * 8);
    // Maximum key values the proof feeds the real envelope: ids are u8 on
    // the wire, so u16::MAX truncates to 0xFF; the nonce is a full u64.
    // Plain == because the wire types deliberately carry no Debug.
    let bevy_id = BevyRequestId::from(u16::MAX);
    let shared_id = SharedRequestId::from(u16::MAX);
    assert!(bevy_id == shared_id);
    let bevy_response: BevyResponseId = bevy_id.receive_from_remote();
    let shared_response: SharedResponseId = shared_id.receive_from_remote();
    assert!(bevy_response == shared_response);
    let bevy_nonce = BevyNonce::from_wire(u64::MAX);
    let shared_nonce = SharedNonce::from_wire(u64::MAX);
    assert!(bevy_nonce == shared_nonce);
    assert_eq!(bevy_nonce.value(), u64::MAX);
}
