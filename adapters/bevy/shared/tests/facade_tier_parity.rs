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
