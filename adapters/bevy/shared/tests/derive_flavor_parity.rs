//! Bevy-flavor derive parity (Rule C emitter policy, Usher seq2599).
//!
//! `Channel`/`Message` derives through `naia_bevy_shared` must expand to
//! bevy-crate paths and produce byte-identical descriptors to the shared
//! flavor. Before the fix this file does not compile: the bevy tier exports
//! no `Channel`/`Message` derive, so the explicit derive paths below fail to
//! resolve. After the fix both flavors expand (same `*_impl` traversal, only
//! the crate-name token differs) and the byte assertions hold.

use naia_bevy_shared::{ChannelDirection, ChannelMode, Message};
use naia_shared::{ChannelKinds, ChannelSettings};

#[derive(naia_bevy_shared::Channel)]
pub struct BevyChannel;

#[derive(naia_shared::Channel)]
pub struct SharedChannel;

#[derive(naia_bevy_shared::Message)]
pub struct BevyPing(u8);

#[derive(naia_shared::Message)]
pub struct SharedPing(u8);

fn settings() -> ChannelSettings {
    ChannelSettings::new(
        ChannelMode::UnorderedUnreliable,
        ChannelDirection::Bidirectional,
    )
}

#[test]
fn bevy_channel_flavor_expands_and_registers() {
    let mut kinds = ChannelKinds::new();
    kinds.add_channel::<BevyChannel>(settings());
    kinds.add_channel::<SharedChannel>(settings());
    assert_eq!(kinds.all_names().len(), 2);
}

#[test]
fn message_flavors_emit_identical_descriptors() {
    // `Message::wire_schema` (not `WireSchema::wire_schema_bytes`): the Message
    // derive emits its own descriptor method; same-shape structs must agree
    // byte-for-byte across flavors.
    assert_eq!(BevyPing::wire_schema(), SharedPing::wire_schema());
}
