//! Insert/payload pairing probes (Usher 42273).
//!
//! Gate-24 hit `an InsertComponent message must carry its own ticked payload`
//! (`remote_world_manager.rs`) in the client during session/main-menu
//! replication. The manager matches each processed `InsertComponent` against
//! an exact `(tick, entity, kind)` payload entry pushed at wire-read time, so
//! a hit means the engine emitted an insert whose key has no payload entry.
//!
//! Each probe below drives [`RemoteEngine`] with the read sequence one
//! session-world interleaving produces (mirroring `world_reader`: every read
//! pushes exactly one payload entry per insert, retransmits included) and
//! then checks every emitted insert against the pushed keys. A probe that
//! finds an unpaired insert fails naming the probe and the offending key —
//! that is the red that identifies the mechanism. A passing probe rules its
//! interleaving out.

use crate::world::local::local_entity::RemoteEntity;
use crate::{
    world::{
        component::component_kinds::ComponentKind, entity::entity_message::EntityMessage,
        sync::RemoteEngine,
    },
    HostType, Tick,
};

struct ComponentType<const T: u8>;

fn component_kind<const T: u8>() -> ComponentKind {
    ComponentKind::from(std::any::TypeId::of::<ComponentType<T>>())
}

/// Mirrors the read side: every `receive_message` call pushes the payload
/// key `world_reader` would push for that read, then checks every emitted
/// insert against the pushed keys.
struct PairingDriver {
    engine: RemoteEngine<RemoteEntity>,
    next_id: u16,
    payloads: Vec<(Tick, u32, ComponentKind)>,
}

impl PairingDriver {
    fn new() -> Self {
        Self {
            engine: RemoteEngine::new(HostType::Client),
            next_id: 1,
            payloads: Vec::new(),
        }
    }

    fn read_insert(&mut self, tick: Tick, entity: u32, kind: ComponentKind) -> u16 {
        let id = self.next_id;
        self.next_id += 1;
        self.read_insert_as(id, tick, entity, kind);
        id
    }

    /// A retransmitted read carries the same message index but the new
    /// packet's tick, and pushes a second payload entry — exactly what
    /// `world_reader` does on a duplicate wire read.
    fn read_insert_as(&mut self, id: u16, tick: Tick, entity: u32, kind: ComponentKind) {
        self.payloads.push((tick, entity, kind));
        self.engine.receive_message(
            id,
            tick,
            EntityMessage::InsertComponent(RemoteEntity::new(entity), kind),
        );
    }

    fn read_bundle(&mut self, tick: Tick, entity: u32, kinds: Vec<ComponentKind>) -> u16 {
        let id = self.next_id;
        self.next_id += 1;
        for kind in &kinds {
            self.payloads.push((tick, entity, *kind));
        }
        self.engine.receive_message(
            id,
            tick,
            EntityMessage::SpawnWithComponents(RemoteEntity::new(entity), kinds),
        );
        id
    }

    fn read(&mut self, tick: Tick, message: EntityMessage<RemoteEntity>) -> u16 {
        let id = self.next_id;
        self.next_id += 1;
        self.engine.receive_message(id, tick, message);
        id
    }

    /// Drain the engine and match every emitted insert against a pushed
    /// payload key. Panics naming the probe and key on the first unpaired
    /// insert — the red-first signal for Usher 42273.
    fn drain_and_check(&mut self, probe: &str) {
        println!("pairing probe running: {probe}");
        let out = self.engine.take_incoming_events();
        for (tick, message) in &out {
            if let EntityMessage::InsertComponent(entity, kind) = message {
                let key = (*tick, entity.value(), *kind);
                let position = self.payloads.iter().position(|entry| *entry == key);
                assert!(
                    position.is_some(),
                    "probe {probe} RED: emitted insert (tick {tick:?}, entity {:?}, kind {kind:?}) has no payload entry; payload keys present: {:?}",
                    entity.value(),
                    self.payloads,
                );
                self.payloads.remove(position.expect("checked above"));
            }
        }
    }
}

/// Insert, remove and re-insert of the same (entity, kind) inside one tick.
#[test]
fn probe_insert_remove_reinsert_same_tick() {
    let mut driver = PairingDriver::new();
    let kind = component_kind::<1>();
    driver.read(1, EntityMessage::Spawn(RemoteEntity::new(7)));
    driver.read_insert(1, 7, kind);
    driver.read(
        1,
        EntityMessage::RemoveComponent(RemoteEntity::new(7), kind),
    );
    driver.read_insert(1, 7, kind);
    driver.drain_and_check("probe_insert_remove_reinsert_same_tick");
}

/// The same message index re-read in a later packet (retransmit): the second
/// read pushes a second payload and re-stamps the buffered tick.
#[test]
fn probe_retransmit_same_id_newer_tick() {
    let mut driver = PairingDriver::new();
    let kind = component_kind::<1>();
    driver.read(1, EntityMessage::Spawn(RemoteEntity::new(7)));
    let id = driver.read_insert(2, 7, kind);
    driver.read_insert_as(id, 5, 7, kind);
    driver.drain_and_check("probe_retransmit_same_id_newer_tick");
}

/// Scope-entry bundle plus a same-tick standalone insert of one bundled kind.
#[test]
fn probe_bundle_plus_insert_same_tick() {
    let mut driver = PairingDriver::new();
    let kind = component_kind::<1>();
    driver.read_bundle(3, 7, vec![kind]);
    driver.read_insert(3, 7, kind);
    driver.drain_and_check("probe_bundle_plus_insert_same_tick");
}

/// Out-of-order arrival with a retransmit before the gap fills.
#[test]
fn probe_out_of_order_with_retransmit() {
    let mut driver = PairingDriver::new();
    let kind = component_kind::<1>();
    driver.read(1, EntityMessage::Spawn(RemoteEntity::new(7)));
    // id 2 is the gap; id 3 arrives first and is held.
    driver.next_id = 3;
    let held = driver.read_insert(3, 7, kind);
    driver.read_insert_as(held, 4, 7, kind);
    driver.next_id = 2;
    driver.read(
        2,
        EntityMessage::RemoveComponent(RemoteEntity::new(7), component_kind::<2>()),
    );
    driver.drain_and_check("probe_out_of_order_with_retransmit");
}

/// Despawn plus respawn reusing the entity id, then a same-kind insert.
#[test]
fn probe_despawn_respawn_id_reuse() {
    let mut driver = PairingDriver::new();
    let kind = component_kind::<1>();
    driver.read(1, EntityMessage::Spawn(RemoteEntity::new(7)));
    driver.read_insert(1, 7, kind);
    driver.read(2, EntityMessage::Despawn(RemoteEntity::new(7)));
    driver.read(3, EntityMessage::Spawn(RemoteEntity::new(7)));
    driver.read_insert(3, 7, kind);
    driver.drain_and_check("probe_despawn_respawn_id_reuse");
}

/// A migration-style channel flush between the message read and the consume
/// must not double-emit the already-read insert.
#[test]
fn probe_flush_between_read_and_consume() {
    let mut driver = PairingDriver::new();
    let kind = component_kind::<1>();
    driver.read(1, EntityMessage::Spawn(RemoteEntity::new(7)));
    driver.read_insert(2, 7, kind);
    driver.engine.flush_entity_channel(RemoteEntity::new(7));
    driver.read_insert(3, 7, kind);
    driver.drain_and_check("probe_flush_between_read_and_consume");
}
