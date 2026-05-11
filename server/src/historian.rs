use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use naia_shared::{
    ComponentKind, EntityAndGlobalEntityConverter, GlobalEntity, Replicate, Tick, WorldRefType,
};

use crate::world::global_world_manager::GlobalWorldManager;

/// Per-entity snapshot: one cloned component value per replicated component.
pub type EntitySnapshot = HashMap<ComponentKind, Box<dyn Replicate>>;

/// Rolling per-tick snapshot buffer for server-side lag compensation.
///
/// # Purpose
///
/// When a client fires a weapon it sends the *client* tick at which it fired.
/// By the time the server processes the packet, the game state has advanced by
/// roughly `RTT/2 + client_interp_offset` ticks. The Historian lets the server
/// "look back" to the tick the client was seeing and perform hit detection
/// against the world state at that moment — preventing the client from being
/// penalised for normal network latency.
///
/// # Usage
///
/// ```no_run
/// # use naia_server::{Server, UserKey};
/// # use naia_shared::{Tick, WorldRefType};
/// # fn example<E: Copy + Eq + std::hash::Hash + Send + Sync, W: WorldRefType<E>>(
/// #     server: &mut Server<E>, world: &W, current_tick: Tick, fire_tick: Tick
/// # ) {
/// // Opt-in once at startup:
/// server.enable_historian(64);
///
/// // Inside tick processing — call before send_all_packets so the snapshot
/// // reflects the state *after* mutation but *before* replication sends:
/// server.record_historian_tick(world, current_tick);
///
/// // On receiving a fire command with client tick T:
/// if let Some(historian) = server.historian() {
///     if let Some(snapshot) = historian.snapshot_at_tick(fire_tick) {
///         // snapshot: HashMap<GlobalEntity, EntitySnapshot>
///         // Iterate snapshot, perform spatial query + hit test, apply damage.
///     }
/// }
/// # }
/// ```
///
/// # Notes
///
/// - Components are cloned via `Replicate::copy_to_box()` at snapshot time —
///   cheap for small structs (position, health), potentially heavy for large
///   components.  Use `enable_historian_filtered` to restrict snapshotting to
///   only the component kinds relevant to your hit detection logic.
/// - The buffer auto-evicts snapshots older than `max_ticks`.
/// - Entity additions/removals are reflected in the next snapshot; the
///   Historian does NOT back-fill missing entities into past snapshots.
pub struct Historian {
    max_ticks: u16,
    snapshots: std::collections::VecDeque<(Tick, HashMap<GlobalEntity, EntitySnapshot>)>,
    /// If `Some`, only components whose `ComponentKind` is in this set are
    /// captured. If `None`, all replicated components are captured (default).
    component_filter: Option<HashSet<ComponentKind>>,
}

impl Historian {
    pub fn new(max_ticks: u16) -> Self {
        Self {
            max_ticks,
            snapshots: std::collections::VecDeque::new(),
            component_filter: None,
        }
    }

    /// Create a Historian that only snapshots component kinds in `filter`.
    ///
    /// This reduces per-tick clone cost on servers with many component types.
    /// Components not in the filter are invisible to `snapshot_at_tick`.
    ///
    /// ```no_run
    /// # use naia_server::Server;
    /// # use naia_shared::ComponentKind;
    /// # fn example<E: Copy + Eq + std::hash::Hash + Send + Sync>(server: &mut Server<E>) {
    /// server.enable_historian_filtered(
    ///     64,
    ///     [ComponentKind::of::<Position>(), ComponentKind::of::<Health>()],
    /// );
    /// # }
    /// # struct Position; impl naia_shared::Replicate for Position {
    /// #     fn copy_to_box(&self) -> Box<dyn naia_shared::Replicate> { unimplemented!() }
    /// #     fn mirror(&mut self, _: &dyn naia_shared::Replicate) {}
    /// # }
    /// # struct Health; impl naia_shared::Replicate for Health {
    /// #     fn copy_to_box(&self) -> Box<dyn naia_shared::Replicate> { unimplemented!() }
    /// #     fn mirror(&mut self, _: &dyn naia_shared::Replicate) {}
    /// # }
    /// ```
    pub fn new_filtered(max_ticks: u16, filter: impl IntoIterator<Item = ComponentKind>) -> Self {
        Self {
            max_ticks,
            snapshots: std::collections::VecDeque::new(),
            component_filter: Some(filter.into_iter().collect()),
        }
    }

    /// Record a snapshot of every replicated entity's components at `tick`.
    ///
    /// Call this after all game-state mutations for the tick have been applied
    /// and before `send_all_packets`, so the snapshot reflects authoritative
    /// game state.
    pub fn record_tick<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        tick: Tick,
        global_world_manager: &GlobalWorldManager,
        global_entity_map: &impl EntityAndGlobalEntityConverter<E>,
        world: &W,
    ) {
        let mut tick_snapshot: HashMap<GlobalEntity, EntitySnapshot> = HashMap::new();

        for &global_entity in global_world_manager.all_global_entities() {
            let Ok(world_entity) = global_entity_map.global_entity_to_entity(&global_entity) else {
                continue;
            };
            let Some(kinds) = global_world_manager.component_kinds(&global_entity) else {
                continue;
            };
            let mut entity_snapshot = EntitySnapshot::new();
            for kind in kinds {
                if let Some(ref filter) = self.component_filter {
                    if !filter.contains(&kind) {
                        continue;
                    }
                }
                if let Some(component_ref) = world.component_of_kind(&world_entity, &kind) {
                    entity_snapshot.insert(kind, component_ref.copy_to_box());
                }
            }
            if !entity_snapshot.is_empty() {
                tick_snapshot.insert(global_entity, entity_snapshot);
            }
        }

        self.snapshots.push_back((tick, tick_snapshot));

        // Evict snapshots older than max_ticks relative to the current tick.
        let max_ticks = self.max_ticks as u32;
        while let Some(&(oldest_tick, _)) = self.snapshots.front() {
            let age = (tick as u32).wrapping_sub(oldest_tick as u32);
            if age > max_ticks {
                self.snapshots.pop_front();
            } else {
                break;
            }
        }
    }

    /// Returns the snapshot for the exact given tick, or `None` if it has
    /// been evicted or never recorded.
    pub fn snapshot_at_tick(&self, tick: Tick) -> Option<&HashMap<GlobalEntity, EntitySnapshot>> {
        for (t, snapshot) in &self.snapshots {
            if *t == tick {
                return Some(snapshot);
            }
        }
        None
    }

    /// Returns the snapshot that was current `time_ago_ms` milliseconds in the
    /// past, given the server's current tick and tick duration.
    ///
    /// Converts the time offset to ticks and delegates to `snapshot_at_tick`.
    /// Clamps to the oldest available tick rather than returning `None` when
    /// `time_ago_ms` is large.
    pub fn snapshot_at_time_ago_ms(
        &self,
        time_ago_ms: u32,
        current_tick: Tick,
        tick_duration_ms: f32,
    ) -> Option<&HashMap<GlobalEntity, EntitySnapshot>> {
        if self.snapshots.is_empty() {
            return None;
        }
        let ticks_ago = (time_ago_ms as f32 / tick_duration_ms).round() as u32;
        let target_tick = (current_tick as u32).wrapping_sub(ticks_ago) as u16;
        // Try exact match first, then fall back to nearest available.
        if let Some(snap) = self.snapshot_at_tick(target_tick) {
            return Some(snap);
        }
        // Return the oldest snapshot as the best approximation.
        self.snapshots.front().map(|(_, s)| s)
    }

    /// Number of snapshots currently retained.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}
