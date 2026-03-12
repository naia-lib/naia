#![cfg(test)]

use crate::world::local::local_entity::RemoteEntity;
use crate::world::sync::config::EngineConfig;
use crate::{
    world::{
        component::component_kinds::ComponentKind, entity::entity_message::EntityMessage,
        sync::RemoteEngine,
    },
    EntityAuthStatus, HostType,
};

struct AssertList {
    asserts: Vec<EntityMessage<RemoteEntity>>,
}

impl AssertList {
    fn new() -> Self {
        Self {
            asserts: Vec::new(),
        }
    }

    fn push(&mut self, msg: EntityMessage<RemoteEntity>) {
        self.asserts.push(msg);
    }

    fn check(&self, engine: &mut RemoteEngine<RemoteEntity>) {
        let out = engine.take_incoming_events();

        assert_eq!(
            self.asserts.len(),
            out.len(),
            "Expected {} messages, got {}",
            self.asserts.len(),
            out.len()
        );

        for (i, assert_msg) in self.asserts.iter().enumerate() {
            assert_eq!(
                assert_msg, &out[i],
                "At index {}, output message: {:?} not equal to expected message: {:?}",
                i, &out[i], assert_msg
            );
        }
    }
}

struct ComponentType<const T: u8>;

fn component_kind<const T: u8>() -> ComponentKind {
    ComponentKind::from(std::any::TypeId::of::<ComponentType<T>>())
}

#[test]
fn engine_basic() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::InsertComponent(entity, comp));
    engine.receive_message(3, EntityMessage::RemoveComponent(entity, comp));
    engine.receive_message(4, EntityMessage::Despawn(entity));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp));
    asserts.push(EntityMessage::RemoveComponent(entity, comp));
    asserts.push(EntityMessage::Despawn(entity));

    asserts.check(&mut engine);
}

#[test]
fn engine_entity_channels_do_not_block() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity_a = RemoteEntity::new(1);
    let entity_b = RemoteEntity::new(2);
    let entity_c = RemoteEntity::new(3);

    engine.receive_message(3, EntityMessage::Spawn(entity_a));
    engine.receive_message(2, EntityMessage::Spawn(entity_b));
    engine.receive_message(1, EntityMessage::Spawn(entity_c));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity_a));
    asserts.push(EntityMessage::Spawn(entity_b));
    asserts.push(EntityMessage::Spawn(entity_c));

    asserts.check(&mut engine);
}

#[test]
fn engine_component_channels_do_not_block() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp_a = component_kind::<1>();
    let comp_b = component_kind::<2>();
    let comp_c = component_kind::<3>();

    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(4, EntityMessage::InsertComponent(entity, comp_a));
    engine.receive_message(3, EntityMessage::InsertComponent(entity, comp_b));
    engine.receive_message(2, EntityMessage::InsertComponent(entity, comp_c));

    // Check order
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp_a));
    asserts.push(EntityMessage::InsertComponent(entity, comp_b));
    asserts.push(EntityMessage::InsertComponent(entity, comp_c));

    asserts.check(&mut engine);
}

#[test]
fn wrap_ordering_simple() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    // Pre-wrap packet (high seq)
    engine.receive_message(65_534, EntityMessage::Spawn(entity));
    // Post-wrap packet (low seq)
    engine.receive_message(0, EntityMessage::InsertComponent(entity, comp));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp));

    asserts.check(&mut engine);
}

#[test]
fn guard_band_flush() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    let near_flush_seq = engine.config.flush_threshold - 2;
    let wrap_beyond_seq = engine.config.flush_threshold + 1;

    engine.receive_message(near_flush_seq, EntityMessage::Spawn(entity));
    engine.receive_message(wrap_beyond_seq, EntityMessage::Spawn(entity));

    // We expect only the later packet to be delivered
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.check(&mut engine);
}

#[test]
fn noop_safe() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    engine.receive_message(10, EntityMessage::Noop);

    let asserts = AssertList::new();
    asserts.check(&mut engine);
}

#[test]
fn backlog_drains_on_prereq_arrival() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    // Insert arrives first, should backlog
    engine.receive_message(6, EntityMessage::InsertComponent(entity, comp));
    // Spawn arrives second
    engine.receive_message(5, EntityMessage::Spawn(entity));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp));

    asserts.check(&mut engine);
}

#[test]
fn entity_despawn_before_spawn() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    // Despawn before spawn
    engine.receive_message(3, EntityMessage::Despawn(entity));
    engine.receive_message(2, EntityMessage::InsertComponent(entity, comp));
    engine.receive_message(1, EntityMessage::Spawn(entity));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
fn component_remove_before_insert() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(3, EntityMessage::RemoveComponent(entity, comp));
    engine.receive_message(2, EntityMessage::InsertComponent(entity, comp));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp));
    asserts.push(EntityMessage::RemoveComponent(entity, comp));
    asserts.check(&mut engine);
}

#[test]
fn empty_drain_safe() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    // Drain when empty
    let out1 = engine.take_incoming_events();
    assert!(out1.is_empty());

    // After guard-band purge scenario – no panic even if drain again
    let out2 = engine.take_incoming_events();
    assert!(out2.is_empty());
}

#[test]
#[ignore]
fn entity_auth_basic() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);

    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));
    engine.receive_message(3, EntityMessage::EnableDelegation(1, entity));
    engine.receive_message(
        4,
        EntityMessage::SetAuthority(1, entity, EntityAuthStatus::Granted),
    );
    engine.receive_message(
        5,
        EntityMessage::SetAuthority(1, entity, EntityAuthStatus::Available),
    );
    engine.receive_message(6, EntityMessage::DisableDelegation(1, entity));
    engine.receive_message(7, EntityMessage::Unpublish(1, entity));
    engine.receive_message(8, EntityMessage::Despawn(entity));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::EnableDelegation(1, entity));
    asserts.push(EntityMessage::SetAuthority(
        1,
        entity,
        EntityAuthStatus::Granted,
    ));
    asserts.push(EntityMessage::SetAuthority(
        1,
        entity,
        EntityAuthStatus::Available,
    ));
    asserts.push(EntityMessage::DisableDelegation(1, entity));
    asserts.push(EntityMessage::Unpublish(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn entity_auth_scrambled() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);

    engine.receive_message(8, EntityMessage::Despawn(entity));
    engine.receive_message(6, EntityMessage::DisableDelegation(5, entity)); // this will never be received
    engine.receive_message(
        4,
        EntityMessage::SetAuthority(3, entity, EntityAuthStatus::Granted),
    ); // this will never be received
    engine.receive_message(2, EntityMessage::Publish(1, entity));
    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(3, EntityMessage::EnableDelegation(2, entity)); // this will never be received
    engine.receive_message(
        5,
        EntityMessage::SetAuthority(3, entity, EntityAuthStatus::Available),
    ); // this will never be received
    engine.receive_message(7, EntityMessage::Unpublish(4, entity)); // this will never be received

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
fn despawn_clears_stale_buffers() {
    // ── Arrange ────────────────────────────────────────────────────────────────
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);
    let comp_a = component_kind::<1>();
    let comp_b = component_kind::<2>();

    // First life ─ valid lifecycle
    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::InsertComponent(entity, comp_a));
    engine.receive_message(4, EntityMessage::Despawn(entity));

    // Stale message that belongs to the *previous* life: must be dropped
    engine.receive_message(3, EntityMessage::InsertComponent(entity, comp_b));

    // Second life ─ fresh epoch
    engine.receive_message(5, EntityMessage::Spawn(entity));

    // ── Assert ────────────────────────────────────────────────────────────────
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp_a));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.push(EntityMessage::Spawn(entity)); // second life
    asserts.check(&mut engine);
}

#[test]
fn component_dense_toggle_sequence() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    // Happy‑path spawn
    engine.receive_message(1, EntityMessage::Spawn(entity));

    // Five out‑of‑order toggles for the SAME component
    engine.receive_message(10, EntityMessage::InsertComponent(entity, comp)); // earliest insert
    engine.receive_message(13, EntityMessage::RemoveComponent(entity, comp)); // legal remove
    engine.receive_message(12, EntityMessage::InsertComponent(entity, comp)); // new insert races in
    engine.receive_message(11, EntityMessage::RemoveComponent(entity, comp)); // stale, must be dropped
    engine.receive_message(14, EntityMessage::InsertComponent(entity, comp)); // buffered, no remove yet

    // Expect: Spawn  → Insert(id 10) → Remove(id 13) → Insert(id 12)
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp));
    asserts.push(EntityMessage::RemoveComponent(entity, comp));
    asserts.push(EntityMessage::InsertComponent(entity, comp));

    asserts.check(&mut engine);
}

#[test]
fn component_backlog_on_entity_a_does_not_block_entity_b() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity_a = RemoteEntity::new(1);
    let entity_b = RemoteEntity::new(2);
    let comp_a = component_kind::<1>();

    // 1. Out‑of‑order insert for Entity A (will backlog until its spawn arrives)
    engine.receive_message(2, EntityMessage::InsertComponent(entity_a, comp_a));

    // 2. Independent spawn for Entity B should *not* be blocked
    engine.receive_message(3, EntityMessage::Spawn(entity_b));

    // Drain: expect only the spawn for Entity B
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity_b));
    asserts.check(&mut engine);

    // 3. Now deliver the missing spawn for Entity A; its backlogged insert must flush
    engine.receive_message(1, EntityMessage::Spawn(entity_a));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity_a));
    asserts.push(EntityMessage::InsertComponent(entity_a, comp_a));
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn entity_auth_illegal_disable_delegation_dropped() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    // Legal path up to Published …
    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));

    // missing (3, EnableDelegationEntity) message never arrives

    // `DisableDelegationEntity` while still Published (never Delegated)
    engine.receive_message(4, EntityMessage::DisableDelegation(1, entity));

    // Follow with an obviously legal message so drain is non‑empty
    engine.receive_message(5, EntityMessage::Despawn(entity));

    // Expected: illegal message is silently dropped
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn entity_auth_illegal_update_authority_dropped() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));

    // missing (3, EnableDelegationEntity) message never arrives

    // Illegal: UpdateAuthority while still Published (never Delegated)
    engine.receive_message(
        4,
        EntityMessage::SetAuthority(1, entity, EntityAuthStatus::Granted),
    );

    engine.receive_message(5, EntityMessage::Despawn(entity));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn entity_auth_illegal_unpublish_while_delegated_dropped() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));
    engine.receive_message(3, EntityMessage::EnableDelegation(1, entity));

    // Illegal: Unpublish while still Delegated
    engine.receive_message(5, EntityMessage::Unpublish(1, entity));

    // Legal sequence to close the loop: revoke delegation then unpublish
    engine.receive_message(4, EntityMessage::DisableDelegation(1, entity));
    engine.receive_message(6, EntityMessage::Despawn(entity));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::EnableDelegation(1, entity));
    asserts.push(EntityMessage::DisableDelegation(1, entity));
    asserts.push(EntityMessage::Unpublish(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn entity_auth_illegal_enable_delegation_while_already_delegated_dropped() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    // Legal path up to Delegated …
    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));
    engine.receive_message(3, EntityMessage::EnableDelegation(1, entity));

    // Illegal: second EnableDelegation while already Delegated
    engine.receive_message(5, EntityMessage::EnableDelegation(1, entity));

    // Close the loop with a legal DisableDelegation and Despawn
    engine.receive_message(4, EntityMessage::DisableDelegation(1, entity));
    engine.receive_message(6, EntityMessage::Despawn(entity));

    // Expect: duplicate EnableDelegation is buffered
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::EnableDelegation(1, entity));
    asserts.push(EntityMessage::DisableDelegation(1, entity));
    asserts.push(EntityMessage::EnableDelegation(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn entity_auth_illegal_publish_while_already_published_dropped() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));

    // Duplicate Publish while still Published
    engine.receive_message(6, EntityMessage::Publish(1, entity));

    engine.receive_message(4, EntityMessage::Despawn(entity));

    // Expect: second Publish is dropped
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn entity_auth_illegal_disable_delegation_while_unpublished_dropped() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));
    engine.receive_message(3, EntityMessage::Unpublish(1, entity));

    // Illegal: DisableDelegation while Unpublished
    engine.receive_message(6, EntityMessage::DisableDelegation(1, entity));

    engine.receive_message(7, EntityMessage::Despawn(entity));

    // Expect: DisableDelegation is dropped
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::Unpublish(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn entity_auth_publish_unpublish_cycle() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);

    // Happy‑path: Spawn → Publish → Unpublish → Publish (again)
    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));
    engine.receive_message(3, EntityMessage::Unpublish(1, entity));
    engine.receive_message(4, EntityMessage::Publish(1, entity));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::Unpublish(1, entity));
    asserts.push(EntityMessage::Publish(1, entity));

    asserts.check(&mut engine);
}

#[test]
fn cross_entity_guard_band() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity_a = RemoteEntity::new(1);
    let entity_b = RemoteEntity::new(2);

    // Sequence just before the sender’s guard‑band flush.
    let near_flush_seq = engine.config.flush_threshold - 1;
    // Low sequence number that would appear right after wrap‑around.
    let low_seq = 10;

    // Near‑wrap spawn for entity A, then low‑ID spawn for entity B.
    engine.receive_message(near_flush_seq, EntityMessage::Spawn(entity_a));
    engine.receive_message(low_seq, EntityMessage::Spawn(entity_b));

    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity_a));
    asserts.push(EntityMessage::Spawn(entity_b));
    asserts.check(&mut engine);
}

/// P‑1 · Engine must **panic** if the same `(MessageIndex, Entity)` is
/// injected twice.  This simulates a failure in the upstream de‑dup layer.
///
/// We deliberately send two *identical* `SpawnEntity` messages with the same
/// ID; the second call **must** trigger the engine’s duplicate‑guard logic.
#[test]
#[should_panic]
fn duplicate_message_id_panics() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    // First‑time acceptance — legal.
    engine.receive_message(1, EntityMessage::Spawn(entity));
    // Re‑injecting the exact same (id, entity, payload) must panic.
    engine.receive_message(1, EntityMessage::Spawn(entity));
}

#[test]
fn max_in_flight_overlap_dropped() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    // 1. Legitimate spawn at a low sequence‑number.
    engine.receive_message(1, EntityMessage::Spawn(entity));

    // 2. Craft an *ambiguous* ID that violates the “< max_in_flight” rule:
    //    Δid == max_in_flight + 1  ⇒  exactly the first overlapping value.
    let overlapping_id: u16 = engine.config.max_in_flight.wrapping_add(1).wrapping_add(1);
    //    (spawn at 1  →  overlapping_id == 1 + 32 768  == 32 769)

    // This insert must be **dropped** by the receiver.
    engine.receive_message(overlapping_id, EntityMessage::InsertComponent(entity, comp));

    // 3. Send an unambiguous in‑order insert to prove the channel still works.
    engine.receive_message(2, EntityMessage::InsertComponent(entity, comp));

    // ── Assert ────────────────────────────────────────────────────────────────
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp)); // only the *second* insert
    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn component_survives_delegation_cycle() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    // Authority cycle with a component toggle in the middle
    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));
    engine.receive_message(3, EntityMessage::EnableDelegation(1, entity));
    engine.receive_message(4, EntityMessage::InsertComponent(entity, comp)); // component appears while delegated
    engine.receive_message(5, EntityMessage::DisableDelegation(1, entity)); // back to Published
    engine.receive_message(6, EntityMessage::RemoveComponent(entity, comp)); // component removed after delegation revoked
    engine.receive_message(7, EntityMessage::Unpublish(1, entity)); // cleanly unpublished

    // Expected: every message—including the component events—survives the auth flip‑flop
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::EnableDelegation(1, entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp));
    asserts.push(EntityMessage::DisableDelegation(1, entity));
    asserts.push(EntityMessage::RemoveComponent(entity, comp));
    asserts.push(EntityMessage::Unpublish(1, entity));

    asserts.check(&mut engine);
}

#[test]
#[ignore]
fn despawn_resets_auth_buffers() {
    // ── Arrange ────────────────────────────────────────────────────────────────
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    // 1st life
    engine.receive_message(1, EntityMessage::Spawn(entity));
    engine.receive_message(2, EntityMessage::Publish(1, entity));
    engine.receive_message(3, EntityMessage::Despawn(entity));

    // 2nd life
    // (4, EntityMessage::SpawnEntity), never arrives

    // Stale auth message arrives
    engine.receive_message(5, EntityMessage::Publish(1, entity));

    // (6, EntityMessage::DespawnEntity), never arrives

    // 3rd life
    // Fresh spawn that should *not* inherit the stale publish
    engine.receive_message(7, EntityMessage::Spawn(entity));

    // ── Assert ────────────────────────────────────────────────────────────────
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity)); // life #1
    asserts.push(EntityMessage::Publish(1, entity));
    asserts.push(EntityMessage::Despawn(entity));
    asserts.push(EntityMessage::Spawn(entity)); // life #3
    asserts.check(&mut engine); // The stale Publish (id 5) must be absent
}

#[test]
fn component_backlog_isolation() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp_a = component_kind::<1>(); // Will backlog first
    let comp_b = component_kind::<2>(); // Must drain immediately

    // 1. Entity spawns normally.
    engine.receive_message(1, EntityMessage::Spawn(entity));

    // 2. Illegal Remove for comp A (inserted = false) → back‑logged inside its channel.
    engine.receive_message(3, EntityMessage::RemoveComponent(entity, comp_a));

    // 3. Independent Insert for comp B with higher ID; must surface right away.
    engine.receive_message(4, EntityMessage::InsertComponent(entity, comp_b));

    // Drain #1: backlog in comp A must NOT block comp B.
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp_b));
    asserts.check(&mut engine);

    // 4. Now deliver the missing Insert for comp A → both events flush in order.
    engine.receive_message(2, EntityMessage::InsertComponent(entity, comp_a));

    // Drain #2: Insert followed by the previously buffered Remove.
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::InsertComponent(entity, comp_a));
    asserts.push(EntityMessage::RemoveComponent(entity, comp_a));
    asserts.check(&mut engine);
}

#[test]
fn component_idempotent_duplicate_drops() {
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);

    let entity = RemoteEntity::new(1);
    let comp = component_kind::<1>();

    // Life 1:
    // (1) Spawn never arrives
    // (2) Insert will arrive late
    // (3) Remove will arrive late
    // (4) Despawn never arrives

    // Life 2:
    // Legitimate lifecycle: Spawn → Insert(id 2) → Remove(id 3)
    engine.receive_message(5, EntityMessage::Spawn(entity));
    engine.receive_message(6, EntityMessage::InsertComponent(entity, comp));

    // 1. Duplicate *older* Insert (id 1) while already inserted → must be discarded.
    engine.receive_message(1, EntityMessage::InsertComponent(entity, comp));

    // 2. Legitimate Remove.
    engine.receive_message(7, EntityMessage::RemoveComponent(entity, comp));

    // 3. Duplicate Remove while already absent → remains back‑logged forever, never drains.
    engine.receive_message(3, EntityMessage::RemoveComponent(entity, comp));

    // Drain: only the non‑duplicate path should surface.
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::InsertComponent(entity, comp));
    asserts.push(EntityMessage::RemoveComponent(entity, comp));
    asserts.check(&mut engine);
}

#[test]
fn large_burst_at_max_in_flight() {
    // ── Arrange ────────────────────────────────────────────────────────────────
    // Create an engine with a *tiny* max_in_flight to keep the test fast.
    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    engine.config = EngineConfig {
        max_in_flight: 15, // 15 < 32768 ⇒ still safe
        flush_threshold: 65521,
    };

    let entity = RemoteEntity::new(99);
    let comp = component_kind::<1>();

    // Spawn first
    engine.receive_message(1, EntityMessage::Spawn(entity));

    // Emit *exactly* max_in_flight packets (IDs 2–16) in perfect order,
    // alternating Insert / Remove to stress the component FSM.
    for i in 0..engine.config.max_in_flight {
        let id = 2 + i;
        let msg = if i % 2 == 0 {
            EntityMessage::InsertComponent(entity, comp)
        } else {
            EntityMessage::RemoveComponent(entity, comp)
        };
        engine.receive_message(id, msg);
    }

    // ── Expect all messages to drain in one shot ──────────────────────────────
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    for i in 0..engine.config.max_in_flight {
        if i % 2 == 0 {
            asserts.push(EntityMessage::InsertComponent(entity, comp));
        } else {
            asserts.push(EntityMessage::RemoveComponent(entity, comp));
        }
    }
    asserts.check(&mut engine);
}

#[test]
fn auth_messages_buffer_until_spawn_epoch() {
    // Test that auth messages (SetAuthority, Publish, etc.) respect the spawn barrier
    // and buffer until Spawn is processed, even when arriving out of order

    let mut engine: RemoteEngine<RemoteEntity> = RemoteEngine::new(HostType::Server);
    let entity = RemoteEntity::new(1);

    // Deliver SetAuthority FIRST with MessageIndex=1 (simulate out-of-order arrival)
    // Note: subcommand_id=0 to match initial next_subcommand_id=0
    engine.receive_message(
        1,
        EntityMessage::SetAuthority(0, entity, EntityAuthStatus::Granted),
    );

    // Assert: no incoming events emitted yet (spawn barrier holds)
    let events = engine.take_incoming_events();
    assert_eq!(
        events.len(),
        0,
        "Auth messages should not be emitted before Spawn"
    );

    // Deliver Spawn with MessageIndex=0 (epoch opener, must have lower id)
    engine.receive_message(0, EntityMessage::Spawn(entity));

    // Assert: Spawn event emitted first, then SetAuthority event
    let mut asserts = AssertList::new();
    asserts.push(EntityMessage::Spawn(entity));
    asserts.push(EntityMessage::SetAuthority(
        0,
        entity,
        EntityAuthStatus::Granted,
    ));
    asserts.check(&mut engine);
}
