//! `build_snapshot` — assemble a `SnapshotWorld<Entity>` from Bevy world
//! state, driven by naia's `SendStateView<Entity>`.
//!
//! This is the canonical server-side snapshot-builder that replaces
//! cyberlith's bespoke `build_snapshot_input` / `SnapshotComponentRegistry`
//! pair. After Tier A `#1` lands, every `Replicate + Component` registered
//! via `Protocol::register_component` / `add_component` automatically has a typed reader in
//! `SnapshotReaderRegistry`, so "replicated ⟺ has a snapshot reader" holds
//! by construction.
//!
//! # Two entry points
//!
//! - **`build_snapshot`** — the normal (dirty-trim) path.  Calls
//!   `send_state_view.needed_live_and_snapshot_entries()` to get only the
//!   entities / component-pairs the next `send_all_packets` actually needs.
//!   This is the production-fast path (MISSION_SNAPSHOT_DIRTY_TRIM).
//!
//! - **`build_snapshot_full`** — the full-scope path.  Calls
//!   `send_state_view.live_entities()` + `send_state_view.required_snapshot_entries()`
//!   to get all in-scope entities and all registered (entity, kind) pairs.
//!   Used for bench / forced-full-snapshot workloads
//!   (`feature = "bench_force_full_snapshot"` in cyberlith).
//!
//! Both return a `SnapshotWorld<Entity>` ready to publish via
//! `SnapshotSender::send`.
//!
//! # Reusable read surface (for `#9`)
//!
//! `SnapshotReaderRegistry` (in `naia-bevy-shared`) exposes a
//! `.read(kind, entity_ref)` method that is the substrate `#9` (the desync
//! harness) will consume.  Callers that need to read a single component off
//! an entity by kind can use that directly.

use bevy_ecs::{entity::Entity, world::World};

use naia_bevy_shared::SnapshotReaderRegistry;
use naia_server::pipeline_actors::SendStateView;

use crate::SnapshotWorld;

/// Build a `SnapshotWorld<Entity>` using the **dirty-trim** path.
///
/// Equivalent to cyberlith's `build_snapshot_input` in the
/// `not(bench_force_full_snapshot)` branch.
pub fn build_snapshot(
    world: &World,
    view: &SendStateView<Entity>,
    readers: &SnapshotReaderRegistry,
) -> SnapshotWorld<Entity> {
    let (live_entities, required) = view.needed_live_and_snapshot_entries();
    assemble(world, &live_entities, &required, readers)
}

/// Build a `SnapshotWorld<Entity>` using the **full-scope** path.
///
/// Equivalent to cyberlith's `build_snapshot_input` in the
/// `bench_force_full_snapshot` branch.
pub fn build_snapshot_full(
    world: &World,
    view: &SendStateView<Entity>,
    readers: &SnapshotReaderRegistry,
) -> SnapshotWorld<Entity> {
    let live_entities = view.live_entities();
    let required = view.required_snapshot_entries();
    assemble(world, &live_entities, &required, readers)
}

fn assemble(
    world: &World,
    live_entities: &[Entity],
    required: &[(Entity, naia_bevy_shared::ComponentKind)],
    readers: &SnapshotReaderRegistry,
) -> SnapshotWorld<Entity> {
    let mut snapshot = SnapshotWorld::new();
    for entity in live_entities {
        snapshot.mark_live(*entity);
    }
    for (entity, kind) in required {
        let Ok(entity_ref) = world.get_entity(*entity) else {
            continue;
        };
        let Some(value) = readers.read(kind, &entity_ref) else {
            // Unregistered kind: can only happen during transition or if a
            // consumer forgets to call `Protocol::register_component::<C>` / `add_component::<C>`. After
            // Tier A `#1` is fully in place this branch is unreachable for
            // cyberlith, because every `add_component` call captures a reader.
            continue;
        };
        snapshot.insert_component(*entity, *kind, value);
    }
    snapshot
}
