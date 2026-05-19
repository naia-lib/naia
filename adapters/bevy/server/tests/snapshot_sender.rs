//! C.6 Addition 5 — `SnapshotSender<E>` Sim-side outbound snapshot facade.
//!
//! Verifies:
//! - `pair()` constructs a matched sender/receiver.
//! - `send` + `take_latest` round-trip.
//! - Latest-wins semantics: a second `send` before `take_latest`
//!   overwrites the first.
//! - Cloning the sender / receiver shares the same slot.

use bevy_ecs::entity::Entity;

use naia_bevy_server::pipeline_actors::SnapshotSender;

use naia_shared::SnapshotWorld;

#[test]
fn pair_send_take_round_trip() {
    let (sender, receiver) = SnapshotSender::<Entity>::pair();
    assert!(!sender.has_pending(), "fresh pair has no pending snapshot");
    assert!(
        receiver.take_latest().is_none(),
        "fresh receiver has nothing to drain",
    );

    sender.send(SnapshotWorld::<Entity>::new());
    assert!(sender.has_pending(), "after send, slot is full");

    let snap = receiver.take_latest();
    assert!(snap.is_some(), "receiver takes the published snapshot");
    assert!(!sender.has_pending(), "slot empty after take_latest");
    assert!(
        receiver.take_latest().is_none(),
        "second take yields None (no new send)",
    );
}

#[test]
fn latest_wins_overwrites_pending_snapshot() {
    let (sender, receiver) = SnapshotSender::<Entity>::pair();
    sender.send(SnapshotWorld::<Entity>::new());
    // Second send before take must replace the first.
    sender.send(SnapshotWorld::<Entity>::new());
    assert!(sender.has_pending());

    // Only one take consumes both — there's only one slot.
    let first = receiver.take_latest();
    assert!(first.is_some(), "first take returns the latest snapshot");
    assert!(receiver.take_latest().is_none(), "no second snapshot queued");
}

#[test]
fn cloned_sender_and_receiver_share_slot() {
    let (sender, receiver) = SnapshotSender::<Entity>::pair();
    let sender_clone = sender.clone();
    let receiver_clone = receiver.clone();

    sender_clone.send(SnapshotWorld::<Entity>::new());
    // Original receiver sees what the clone published.
    let snap = receiver.take_latest();
    assert!(
        snap.is_some(),
        "receiver sees snapshot published through cloned sender",
    );

    sender.send(SnapshotWorld::<Entity>::new());
    // Cloned receiver drains it.
    assert!(
        receiver_clone.take_latest().is_some(),
        "cloned receiver shares the same backing slot",
    );
}
