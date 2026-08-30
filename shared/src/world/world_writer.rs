use std::{
    clone::Clone,
    collections::{HashMap, VecDeque},
    hash::Hash,
};

use crate::{
    messages::channels::senders::indexed_message_writer::IndexedMessageWriter,
    world::{
        entity::entity_converters::GlobalWorldManagerType, host::host_world_manager::CommandId,
        local::local_world_manager::LocalWorldManager,
        update::global_diff_handler::GlobalDiffHandler,
        update::global_entity_index::GlobalEntityIndex,
    },
    BitWrite, BitWriter, CachedComponentUpdate, ComponentKind, ComponentKinds, DiffMask,
    EntityAndGlobalEntityConverter, EntityCommand, EntityMessage, EntityMessageType, GlobalEntity,
    Instant, LocalEntityAndGlobalEntityConverter, MessageIndex, PacketIndex, Replicate, Serde,
    WorldRefType,
};

/// MISSION_TICK_FLOOR Lever 3: per-(entity) update plan entry. Each tuple is
/// `(ComponentKind, kind_bit, DiffMask)`. Ordered by `kind_bit` ascending
/// (insertion order from `prepare_send_job`'s dirty-word scan). Vec instead of
/// HashMap: eliminates per-(entity,user,tick) HashMap construction + key-collect
/// + lookup + remove allocations in the hot `write_update` path.
///
/// The `u16` is the `kind_bit`; the `DiffMask` is the **frozen** per-property
/// mask captured at the freeze point — NOT a live fetch. Threading the frozen
/// mask is what lets the lagged send worker serialize a self-contained job
/// without reading concurrently-mutated per-user diff state. See
/// `_AGENTS/L3_PURE_SENDJOB_FIX_HANDOFF.md`.
pub type UpdateKinds = Vec<(ComponentKind, u16, DiffMask)>;

/// Per-tick counters for the packet-write path.
/// Enabled via `bench_instrumentation`.
///
/// - `N_SCOPE_ENTRY_SPAWNS`: SpawnWithComponents commands actually written (not Noop'd) per tick.
#[cfg(feature = "bench_instrumentation")]
pub mod bench_write_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    #[doc(hidden)]
    pub static N_SCOPE_ENTRY_SPAWNS: AtomicU64 = AtomicU64::new(0);
    /// PATH A wire-cache hits: bytes replayed from cache, zero ECS reads.
    #[doc(hidden)]
    pub static N_PATH_A_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
    /// PATH A wire-cache misses: ECS read + serialize + store into cache.
    #[doc(hidden)]
    pub static N_PATH_A_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

    /// Resets all write counters to zero.
    pub fn reset() {
        N_SCOPE_ENTRY_SPAWNS.store(0, Ordering::Relaxed);
        N_PATH_A_CACHE_HITS.store(0, Ordering::Relaxed);
        N_PATH_A_CACHE_MISSES.store(0, Ordering::Relaxed);
    }
    /// Returns the number of SpawnWithComponents commands written this tick.
    pub fn snapshot_spawns() -> u64 {
        N_SCOPE_ENTRY_SPAWNS.load(Ordering::Relaxed)
    }
    /// Returns (hits, misses) for the PATH A wire-cache since last reset.
    pub fn snapshot_path_a() -> (u64, u64) {
        (
            N_PATH_A_CACHE_HITS.load(Ordering::Relaxed),
            N_PATH_A_CACHE_MISSES.load(Ordering::Relaxed),
        )
    }
}

/// Pre-ECS-snapshot for UserDependent components (those with EntityProperty fields).
/// Built once per tick per component — keyed by (GlobalEntity, ComponentKind).
/// First user to write a UserDependent component reads from ECS and populates this map;
/// subsequent users serialize from the snapshot, touching ECS zero times.
pub type SnapshotMap = HashMap<(GlobalEntity, ComponentKind), Box<dyn Replicate>>;

/// Why a planned component update was dropped between the freeze that planned
/// it and the transmit that would have serialized it.
///
/// All three reasons are the same race in different axes. `prepare_send_job`
/// FREEZES a plan one tick before `transmit_and_pump` writes it, and the
/// gameplay thread keeps running in that window. Every variant here is
/// legitimate runtime state, never an error -- the client is about to be told
/// the truth by some other means in each case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateDropReason {
    /// The entity was despawned in the freeze->transmit window. Its components
    /// are gone from the World, so serializing them would panic
    /// (`component_of_kind` -> `expect`), and replaying PATH A's cached bytes
    /// would be worse: the cache is keyed by `GlobalEntityIndex`, which is
    /// recyclable, so a stale hit can emit ANOTHER entity's bytes. The update
    /// is moot -- the client is about to receive the Despawn.
    EntityDespawned,
    /// Every planned component kind has since been removed from the entity.
    ///
    /// `write_update` below drops stale kinds on its own, correctly, but not
    /// for free: the UpdateContinue bit + LocalEntity (~20 bits) are already
    /// committed by then and nothing rolls them back. An entity whose planned
    /// kinds are ALL stale therefore contributes pure framing and zero payload,
    /// leaving `has_written` false. Under heavy scope churn enough of those
    /// accumulate to fill the packet, and `write_commands` then sees a full
    /// packet with `has_written == false` and takes the "this component is too
    /// big to ever send" panic path -- which does not describe the state at all.
    /// Measured (world editor 69o, refined tile scope): 167 entities, 167
    /// stale, 3437 of 3440 bits spent on headers, 5081 spawns starved behind it.
    /// Catching it here makes the wasted header not exist.
    AllKindsStale,
    /// The host held authority over a delegated entity when the update was
    /// marked dirty, and lost it -- released, revoked or denied -- before the
    /// update was transmitted. Serializing would reach
    /// `DelegatedProperty::write` / `DelegatedRelation::write`, both of which
    /// panic when the host cannot write, taking the process down over a
    /// legitimate ordering. The update is moot: whoever holds authority now
    /// owns the authoritative value, and this host will receive it.
    ///
    /// This is a client-side drop in practice -- `can_write()` is true for
    /// every server auth status -- but both production `GlobalWorldManagerType`
    /// implementors report a real status, so the check is not skipped anywhere.
    AuthorityLost,
}

/// The single decision point for the three freeze->transmit races above.
///
/// Folded into one function so the reasons stay enumerable: `UpdateDropReason`
/// is what [`drop_counters`] keys on, which is what lets a test assert its
/// scenario actually REACHED the state under test rather than merely passing.
fn planned_update_drop_reason<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
    world: &W,
    world_entity: &E,
    global_entity: &GlobalEntity,
    kinds: &UpdateKinds,
    global_world_manager: &dyn GlobalWorldManagerType,
) -> Option<UpdateDropReason> {
    if !world.has_entity(world_entity) {
        return Some(UpdateDropReason::EntityDespawned);
    }

    if !kinds
        .iter()
        .any(|(kind, _, _)| world.has_component_of_kind(world_entity, kind))
    {
        return Some(UpdateDropReason::AllKindsStale);
    }

    // `None` means the entity has no delegation authority state to consult, so
    // there is no constraint to violate. Note the trait default is `None`, i.e.
    // fail-open: an implementor that does not override `entity_auth_status`
    // gets no guard at all. Both production implementors override it.
    if let Some(auth_status) = global_world_manager.entity_auth_status(global_entity) {
        if !auth_status.can_write() {
            return Some(UpdateDropReason::AuthorityLost);
        }
    }

    None
}

/// Test-only tally of which [`UpdateDropReason`] guards actually fired.
///
/// A harness test that never reaches the state it claims to cover passes just
/// as green as one that does -- a coverage illusion this crate has been bitten
/// by more than once. These counters turn "the test passed" into "the test
/// reached the state under test": assert the count, not just the outcome.
///
/// Thread-local, because the test binary runs tests in parallel on separate
/// threads and `write_updates` runs synchronously on the calling thread; a
/// process-wide counter would cross-talk. Compiled away entirely outside
/// `cfg(test)`.
pub(crate) mod drop_counters {
    use super::UpdateDropReason;

    #[cfg(test)]
    thread_local! {
        static COUNTS: std::cell::Cell<[usize; 3]> = const { std::cell::Cell::new([0; 3]) };
    }

    #[cfg(test)]
    fn slot(reason: UpdateDropReason) -> usize {
        match reason {
            UpdateDropReason::EntityDespawned => 0,
            UpdateDropReason::AllKindsStale => 1,
            UpdateDropReason::AuthorityLost => 2,
        }
    }

    #[inline]
    pub(crate) fn record(reason: UpdateDropReason) {
        #[cfg(test)]
        COUNTS.with(|counts| {
            let mut current = counts.get();
            current[slot(reason)] += 1;
            counts.set(current);
        });
        #[cfg(not(test))]
        let _ = reason;
    }

    /// How many times `reason` fired on this thread since the last [`reset`].
    #[cfg(test)]
    pub(crate) fn count(reason: UpdateDropReason) -> usize {
        COUNTS.with(|counts| counts.get()[slot(reason)])
    }

    /// Zeroes this thread's tallies. Call at the top of a test that asserts on
    /// counts, so an earlier test on a reused thread cannot inflate them.
    #[cfg(test)]
    pub(crate) fn reset() {
        COUNTS.with(|counts| counts.set([0; 3]));
    }
}

pub struct WorldWriter;

impl WorldWriter {
    fn write_command_id(
        writer: &mut dyn BitWrite,
        last_id_opt: &mut Option<CommandId>,
        current_id: &CommandId,
    ) {
        IndexedMessageWriter::write_message_index(writer, last_id_opt, current_id);
        *last_id_opt = Some(*current_id);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn write_into_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        component_kinds: &ComponentKinds,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        global_diff_handler: Option<&GlobalDiffHandler>,
        world_manager: &mut LocalWorldManager,
        has_written: &mut bool,
        world_events: &mut VecDeque<(CommandId, EntityCommand)>,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        snapshot_map: Option<&SnapshotMap>,
    ) {
        // write entity updates
        Self::write_updates(
            component_kinds,
            now,
            writer,
            packet_index,
            world,
            global_world_manager,
            global_diff_handler,
            world_manager,
            has_written,
            update_list,
            snapshot_map,
        );

        // write entity commands
        Self::write_commands(
            component_kinds,
            now,
            writer,
            packet_index,
            world,
            entity_converter,
            global_world_manager,
            world_manager,
            has_written,
            world_events,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn write_commands<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        component_kinds: &ComponentKinds,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_manager: &mut LocalWorldManager,
        has_written: &mut bool,
        next_send_commands: &mut VecDeque<(CommandId, EntityCommand)>,
    ) {
        let mut last_counted_id: Option<MessageIndex> = None;
        let mut last_written_id: Option<MessageIndex> = None;

        loop {
            if next_send_commands.is_empty() {
                break;
            }

            // check that we can write the next message
            let mut counter = writer.counter();
            // write CommandContinue bit
            true.ser(&mut counter);
            // write data
            Self::write_command(
                component_kinds,
                world,
                entity_converter,
                global_world_manager,
                world_manager,
                packet_index,
                &mut counter,
                &mut last_counted_id,
                false,
                next_send_commands,
            );
            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of component being too big
                if !*has_written {
                    Self::warn_overflow_command(
                        component_kinds,
                        counter.bits_needed(),
                        writer.bits_free(),
                        next_send_commands,
                    );
                }
                break;
            }

            *has_written = true;

            // optimization
            world_manager.insert_sent_command_packet(packet_index, now.clone());

            // write CommandContinue bit
            true.ser(writer);
            // write data
            Self::write_command(
                component_kinds,
                world,
                entity_converter,
                global_world_manager,
                world_manager,
                packet_index,
                writer,
                &mut last_written_id,
                true,
                next_send_commands,
            );

            // pop command we've written
            next_send_commands.pop_front();
        }

        // Finish commands by writing false CommandContinue bit
        writer.release_bits(1);
        false.ser(writer);
    }

    #[allow(clippy::too_many_arguments)]
    fn write_command<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        component_kinds: &ComponentKinds,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_manager: &mut LocalWorldManager,
        packet_index: &PacketIndex,
        writer: &mut dyn BitWrite,
        last_written_id: &mut Option<CommandId>,
        is_writing: bool,
        next_send_commands: &mut VecDeque<(CommandId, EntityCommand)>,
    ) {
        let (command_id, command) = next_send_commands.front().unwrap();

        // info!("Writing (command_id: {:?}), command {:?} into packet {:?}", command_id, command, packet_index);

        // write command id
        Self::write_command_id(writer, last_written_id, command_id);

        match command {
            EntityCommand::Spawn(global_entity) => {
                EntityMessageType::Spawn.ser(writer);

                // get host entity
                let host_entity = world_manager
                    .entity_converter()
                    .global_entity_to_host_entity(global_entity)
                    .unwrap();

                // write host entity
                host_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::Spawn(host_entity.copy_to_owned()),
                    );
                }
            }
            EntityCommand::SpawnWithComponents(global_entity, comp_kind_list) => {
                let Some(world_entity) =
                    entity_converter.global_entity_to_entity(global_entity).ok()
                else {
                    EntityMessageType::Noop.ser(writer);
                    if is_writing {
                        world_manager.record_command_written(
                            packet_index,
                            command_id,
                            EntityMessage::Noop,
                        );
                    }
                    return;
                };

                let has_global = world_manager.has_global_entity(global_entity);
                if !has_global {
                    // LEGITIMATE race: a Despawn superseded this Spawn in the
                    // same window (`host_engine` removes the channel on Despawn
                    // while the queued Spawn still drains). Degrade to Noop —
                    // the client never sees a corpse it would immediately kill.
                    EntityMessageType::Noop.ser(writer);
                    if is_writing {
                        world_manager.record_command_written(
                            packet_index,
                            command_id,
                            EntityMessage::Noop,
                        );
                    }
                    return;
                }

                let present_count = comp_kind_list
                    .iter()
                    .filter(|k| world.has_component_of_kind(&world_entity, k))
                    .count();
                let all_present = present_count == comp_kind_list.len();
                if !all_present {
                    // Two distinct states reach here (measured 2026-06-10,
                    // cyberlith f5_world_disconnect vs the baked-floor bug):
                    //
                    // - NONE of the kinds present: the sim entity was torn down
                    //   wholesale in this same window (disconnect/despawn race —
                    //   the snapshot builder skips a despawned entity's every
                    //   component while the queued Spawn still drains and the
                    //   global record outlives the sim entity by a beat). Same
                    //   legitimacy class as the `!has_global` arm above:
                    //   converge to a quiet Noop; a Despawn/teardown follows.
                    //
                    // - SOME present, some missing: genuine needed-set or
                    //   snapshot-registry under-supply (e.g. a Replicate
                    //   component missing its snapshot registration — the
                    //   2026-06-10 "baked floor vanished" bug). The Noop below
                    //   is RECORDED as this command's delivery, so the spawn is
                    //   permanently lost for this peer. Loud in debug/test AND
                    //   release (downstream workspaces disable debug-assertions
                    //   even in dev profiles).
                    //
                    // A needed-set bug that under-supplied an entity WHOLESALE
                    // would look like the first state — naia cannot tell them
                    // apart here, so hosts keep their own seam guard (cyberlith:
                    // the `build_snapshot_input` warn-once + the d4 floor
                    // delivery gates).
                    let partial = present_count > 0;
                    debug_assert!(
                        !partial,
                        "SpawnWithComponents: entity {:?} is host-tracked but only \
                         {present_count}/{} component kinds are in the snapshot world — \
                         needed-set/snapshot-registry under-supply (would silently \
                         drop the spawn)",
                        global_entity,
                        comp_kind_list.len(),
                    );
                    if partial && is_writing {
                        log::warn!(
                            "SpawnWithComponents for {:?} degraded to a TERMINAL Noop: \
                             only {}/{} component kinds present in the snapshot world \
                             (needed-set or snapshot-registry under-supply) — \
                             the entity will never spawn on this peer",
                            global_entity,
                            present_count,
                            comp_kind_list.len(),
                        );
                    }
                    EntityMessageType::Noop.ser(writer);
                    if is_writing {
                        world_manager.record_command_written(
                            packet_index,
                            command_id,
                            EntityMessage::Noop,
                        );
                    }
                    return;
                }

                EntityMessageType::SpawnWithComponents.ser(writer);

                let host_entity = world_manager
                    .entity_converter()
                    .global_entity_to_host_entity(global_entity)
                    .unwrap();
                host_entity.copy_to_owned().ser(writer);

                let count = comp_kind_list.len() as u8;
                count.ser(writer);

                {
                    let mut converter = world_manager.entity_converter_mut(global_world_manager);
                    for component_kind in comp_kind_list.iter() {
                        world
                            .component_of_kind(&world_entity, component_kind)
                            .expect("Component does not exist in World")
                            .write(component_kinds, writer, &mut converter);
                    }
                }

                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::SpawnWithComponents(
                            host_entity.copy_to_owned(),
                            comp_kind_list.clone(),
                        ),
                    );
                    #[cfg(feature = "bench_instrumentation")]
                    bench_write_counters::N_SCOPE_ENTRY_SPAWNS
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
            EntityCommand::Despawn(global_entity) => {
                EntityMessageType::Despawn.ser(writer);

                // get local entity
                let local_entity = world_manager
                    .entity_converter()
                    .global_entity_to_owned_entity(global_entity)
                    .unwrap();

                // write local entity
                local_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::Despawn(local_entity),
                    );
                }
            }
            EntityCommand::InsertComponent(global_entity, component_kind) => {
                // get world entity
                let Some(world_entity) =
                    entity_converter.global_entity_to_entity(global_entity).ok()
                else {
                    EntityMessageType::Noop.ser(writer);
                    if is_writing {
                        world_manager.record_command_written(
                            packet_index,
                            command_id,
                            EntityMessage::Noop,
                        );
                    }
                    return;
                };

                let insert_has_global = world_manager.has_global_entity(global_entity);
                // Same split as SpawnWithComponents: `!has_global` is the
                // legitimate despawn-race Noop; `has_global && !present` is a
                // needed-set under-supply that would silently drop the insert.
                let insert_present =
                    insert_has_global && world.has_component_of_kind(&world_entity, component_kind);
                debug_assert!(
                    !insert_has_global || insert_present,
                    "InsertComponent: entity {:?} is host-tracked but component {:?} \
                     is missing from the snapshot world — needed-set under-supply \
                     (would silently drop the insert)",
                    global_entity,
                    component_kind,
                );
                if !insert_present {
                    // Same terminal-loss warn as SpawnWithComponents: only the
                    // under-supply case is loud; the `!has_global` despawn race
                    // is a legitimate quiet Noop.
                    if is_writing && insert_has_global {
                        log::warn!(
                            "InsertComponent for {:?} ({:?}) degraded to a TERMINAL \
                             Noop: component missing from the snapshot world \
                             (needed-set or snapshot-registry under-supply) — \
                             the insert will never reach this peer",
                            global_entity,
                            component_kind,
                        );
                    }
                    EntityMessageType::Noop.ser(writer);

                    // if we are actually writing this packet
                    if is_writing {
                        // add it to command record
                        world_manager.record_command_written(
                            packet_index,
                            command_id,
                            EntityMessage::Noop,
                        );
                    }
                } else {
                    EntityMessageType::InsertComponent.ser(writer);

                    // get local entity
                    let local_entity = world_manager
                        .entity_converter()
                        .global_entity_to_owned_entity(global_entity)
                        .unwrap();

                    // write local entity
                    local_entity.ser(writer);

                    {
                        let mut converter =
                            world_manager.entity_converter_mut(global_world_manager);

                        // write component payload
                        world
                            .component_of_kind(&world_entity, component_kind)
                            .expect("Component does not exist in World")
                            .write(component_kinds, writer, &mut converter);
                    }

                    // if we are actually writing this packet
                    if is_writing {
                        // add it to command record
                        world_manager.record_command_written(
                            packet_index,
                            command_id,
                            EntityMessage::InsertComponent(local_entity, *component_kind),
                        );
                    }
                }
            }
            EntityCommand::RemoveComponent(global_entity, component_kind) => {
                if !world_manager.has_global_entity(global_entity) {
                    EntityMessageType::Noop.ser(writer);

                    // if we are actually writing this packet
                    if is_writing {
                        // add it to command record
                        world_manager.record_command_written(
                            packet_index,
                            command_id,
                            EntityMessage::Noop,
                        );
                    }
                } else {
                    EntityMessageType::RemoveComponent.ser(writer);

                    // get local entity
                    let local_entity = world_manager
                        .entity_converter()
                        .global_entity_to_owned_entity(global_entity)
                        .unwrap();

                    // write local entity
                    local_entity.ser(writer);

                    // write component kind
                    component_kind.ser(component_kinds, writer);

                    // if we are writing to this packet, add it to record
                    if is_writing {
                        world_manager.record_command_written(
                            packet_index,
                            command_id,
                            EntityMessage::RemoveComponent(local_entity, *component_kind),
                        );
                    }
                }
            }
            EntityCommand::Publish(sub_id_opt, global_entity) => {
                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("Publish command must have a CommandId");
                };

                // write message type
                EntityMessageType::Publish.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get local entity
                let local_entity = world_manager
                    .entity_converter()
                    .global_entity_to_owned_entity(global_entity)
                    .unwrap();

                // write local entity
                local_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::Publish(*sub_id, local_entity),
                    );
                }
            }
            EntityCommand::Unpublish(sub_id_opt, global_entity) => {
                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("Unpublish command must have a CommandId");
                };

                // write message type
                EntityMessageType::Unpublish.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get local entity
                let local_entity = world_manager
                    .entity_converter()
                    .global_entity_to_owned_entity(global_entity)
                    .unwrap();

                // write local entity
                local_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::Unpublish(*sub_id, local_entity),
                    );
                }
            }
            EntityCommand::EnableDelegation(sub_id_opt, global_entity) => {
                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("EnableDelegation command must have a CommandId");
                };

                // write message type
                EntityMessageType::EnableDelegation.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get local entity
                let local_entity = world_manager
                    .entity_converter()
                    .global_entity_to_owned_entity(global_entity)
                    .unwrap();

                local_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::EnableDelegation(*sub_id, local_entity),
                    );
                }
            }
            EntityCommand::DisableDelegation(sub_id_opt, global_entity) => {
                // this command is only ever sent by the server, regarding server-owned entities, to clients

                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("DisableDelegation command must have a CommandId");
                };

                // write message type
                EntityMessageType::DisableDelegation.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get host entity
                let host_entity = world_manager
                    .entity_converter()
                    .global_entity_to_host_entity(global_entity)
                    .unwrap();

                // write host entity
                host_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::DisableDelegation(*sub_id, host_entity.copy_to_owned()),
                    );
                }
            }
            EntityCommand::SetAuthority(sub_id_opt, global_entity, auth_status) => {
                // this command is sent by the server to clients (for both server-owned and client-owned entities)

                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("SetAuthority command must have a CommandId");
                };

                // write message type
                EntityMessageType::SetAuthority.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get remote entity (client always reads SetAuthority as RemoteEntity)
                // Try RemoteEntity first (for client-owned entities on server), fall back to HostEntity if needed
                let remote_entity = world_manager
                    .entity_converter()
                    .global_entity_to_remote_entity(global_entity)
                    .or_else(|_| {
                        // Fallback: if it's a HostEntity, convert it to RemoteEntity
                        // This handles the case where server-owned entities are sent as SetAuthority
                        world_manager
                            .entity_converter()
                            .global_entity_to_host_entity(global_entity)
                            .map(|he| he.to_remote())
                    })
                    .unwrap_or_else(|_| {
                        panic!(
                            "SetAuthority: Cannot convert GlobalEntity {:?} to RemoteEntity or HostEntity",
                            global_entity
                        );
                    });

                // write remote entity
                remote_entity.ser(writer);

                // write auth status
                auth_status.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::SetAuthority(
                            *sub_id,
                            remote_entity.copy_to_owned(),
                            *auth_status,
                        ),
                    );
                }
            }

            // below are response-type commands
            EntityCommand::RequestAuthority(sub_id_opt, global_entity) => {
                // this command is only ever sent by clients, regarding server-owned entities, to server

                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("RequestAuthority command must have a CommandId");
                };

                // write message type
                EntityMessageType::RequestAuthority.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get remote entity
                let remote_entity = world_manager
                    .entity_converter()
                    .global_entity_to_remote_entity(global_entity)
                    .unwrap();

                // write remote entity
                remote_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::RequestAuthority(*sub_id, remote_entity.copy_to_owned()),
                    );
                }
            }
            EntityCommand::ReleaseAuthority(sub_id_opt, global_entity) => {
                // this command is only ever sent by clients, regarding server-owned entities, to server

                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("ReleaseAuthority command must have a CommandId");
                };

                // write message type
                EntityMessageType::ReleaseAuthority.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get local entity
                // NOTE: this is actually valid because it should be possible to ReleaseAuthority right after EnableDelegation, so that auth isn't automatically set to Granted
                let local_entity = world_manager
                    .entity_converter()
                    .global_entity_to_owned_entity(global_entity)
                    .unwrap();

                // write local entity
                local_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::ReleaseAuthority(*sub_id, local_entity),
                    );
                }
            }
            EntityCommand::EnableDelegationResponse(sub_id_opt, global_entity) => {
                // this command is only ever sent by clients, regarding server-owned entities, to server

                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("EnableDelegationResponse command must have a CommandId");
                };

                // write message type
                EntityMessageType::EnableDelegationResponse.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get remote entity
                let remote_entity = world_manager
                    .entity_converter()
                    .global_entity_to_remote_entity(global_entity)
                    .unwrap();

                // write remote entity
                remote_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::EnableDelegationResponse(
                            *sub_id,
                            remote_entity.copy_to_owned(),
                        ),
                    );
                }
            }
            EntityCommand::MigrateResponse(
                sub_id_opt,
                _global_entity,
                old_remote_entity,
                new_host_entity_value,
            ) => {
                // this command is only ever sent by the server, regarding newly delegated server-owned entities, to clients

                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("MigrateResponse command must have a CommandId");
                };

                // write message type
                EntityMessageType::MigrateResponse.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // Convert server's RemoteEntity to client's HostEntity (same value, different type)
                // The client can look this up in its entity_map!
                let client_host_entity = old_remote_entity.to_host();
                client_host_entity.ser(writer);

                // write new remote entity (what the client will create)
                let new_remote_entity = new_host_entity_value.to_remote();
                new_remote_entity.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::MigrateResponse(
                            *sub_id,
                            client_host_entity.copy_to_owned(),
                            new_remote_entity,
                        ),
                    );
                }
            }
        }
    }

    fn warn_overflow_command(
        component_kinds: &ComponentKinds,
        bits_needed: u32,
        bits_free: u32,
        next_send_commands: &VecDeque<(CommandId, EntityCommand)>,
    ) {
        let (_command_id, command) = next_send_commands.front().unwrap();

        match command {
            EntityCommand::Spawn(_entity) => {
                panic!(
                    "Packet Write Error: Blocking overflow detected! Entity Spawn message requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommend slimming down these Components."
                )
            }
            EntityCommand::SpawnWithComponents(_entity, _kinds) => {
                panic!(
                    "Packet Write Error: Blocking overflow detected! SpawnWithComponents message requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommend slimming down these Components."
                )
            }
            EntityCommand::InsertComponent(_entity, component_kind) => {
                let component_name = component_kinds.kind_to_name(component_kind);
                panic!(
                    "Packet Write Error: Blocking overflow detected! Component Insertion message of type `{component_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! This condition should never be reached, as large Messages should be Fragmented in the Reliable channel"
                )
            }
            EntityCommand::Publish(_, _)
            | EntityCommand::Unpublish(_, _)
            | EntityCommand::EnableDelegation(_, _)
            | EntityCommand::EnableDelegationResponse(_, _)
            | EntityCommand::DisableDelegation(_, _)
            | EntityCommand::RequestAuthority(_, _)
            | EntityCommand::ReleaseAuthority(_, _)
            | EntityCommand::SetAuthority(_, _, _)
            | EntityCommand::MigrateResponse(_, _, _, _) => {
                panic!(
                    "Packet Write Error: Blocking overflow detected! Authority/delegation command requires {bits_needed} bits, but packet only has {bits_free} bits available! These messages should be small and not cause overflow."
                )
            }
            _ => {
                panic!(
                    "Packet Write Error: Blocking overflow detected! Command requires {bits_needed} bits, but packet only has {bits_free} bits available! This message should never display..."
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        component_kinds: &ComponentKinds,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        global_world_manager: &dyn GlobalWorldManagerType,
        global_diff_handler: Option<&GlobalDiffHandler>,
        world_manager: &mut LocalWorldManager,
        has_written: &mut bool,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        snapshot_map: Option<&SnapshotMap>,
    ) {
        let mut i = 0;
        while i < update_list.len() {
            // Copy the Copy fields before the mutable borrow of kinds
            let (global_entity, entity_idx, world_entity) = {
                let (ge, idx, we, _) = &update_list[i];
                (*ge, *idx, *we)
            };

            if let Some(reason) = planned_update_drop_reason(
                world,
                &world_entity,
                &global_entity,
                &update_list[i].3,
                global_world_manager,
            ) {
                drop_counters::record(reason);
                update_list[i].3.clear();
                i += 1;
                continue;
            }

            let local_entity = world_manager
                .entity_converter()
                .global_entity_to_owned_entity(&global_entity)
                .unwrap();

            // check that we can at least write a LocalEntity and a ComponentContinue bit
            let mut counter = writer.counter();
            // reserve ComponentContinue bit
            counter.write_bit(true);
            // write UpdateContinue bit
            counter.write_bit(true);
            // write LocalEntity
            local_entity.ser(&mut counter);

            if counter.overflowed() {
                break;
            }

            // reserve ComponentContinue bit
            writer.reserve_bits(1);
            // write UpdateContinue bit
            true.ser(writer);
            // write LocalEntity
            local_entity.ser(writer);

            // write Components
            let kinds = &mut update_list[i].3;
            Self::write_update(
                component_kinds,
                now,
                world,
                global_world_manager,
                global_diff_handler,
                world_manager,
                packet_index,
                writer,
                &global_entity,
                entity_idx,
                &world_entity,
                has_written,
                kinds,
                snapshot_map,
            );

            // write ComponentContinue finish bit, release
            writer.release_bits(1);
            false.ser(writer);

            i += 1;
        }

        // Remove fully-written entries (all component kinds serialized).
        update_list.retain(|(_, _, _, kinds)| !kinds.is_empty());

        // write EntityContinue finish bit, release
        writer.release_bits(1);
        false.ser(writer);
    }

    /// For a given entity, write component value updates into a packet.
    /// Implements two principled serialization paths:
    /// - PATH A (UserIndependent): components without EntityProperty fields share
    ///   a CachedComponentUpdate keyed by DiffMask. First user after mutation pays
    ///   one ECS read + serialize; all others replay the cached bytes.
    /// - PATH B (UserDependent): components with EntityProperty fields serialize
    ///   per-user local entity IDs. ECS is read once per component per tick into
    ///   snapshot_map; all users serialize from the snapshot, not ECS.
    #[allow(clippy::too_many_arguments)]
    fn write_update<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        component_kinds: &ComponentKinds,
        now: &Instant,
        world: &W,
        global_world_manager: &dyn GlobalWorldManagerType,
        global_diff_handler: Option<&GlobalDiffHandler>,
        world_manager: &mut LocalWorldManager,
        packet_index: &PacketIndex,
        writer: &mut BitWriter,
        global_entity: &GlobalEntity,
        entity_idx: GlobalEntityIndex,
        world_entity: &E,
        has_written: &mut bool,
        kinds: &mut UpdateKinds,
        snapshot_map: Option<&SnapshotMap>,
    ) {
        // Vec<(ComponentKind, kind_bit, DiffMask)> — iterate in insertion order
        // (kind_bit ascending from prepare_send_job). Track how many entries we
        // successfully serialize; drain that prefix after the loop. Entries left
        // in the Vec (overflow case) are picked up in the next build_one_packet
        // call. Eliminates the per-(entity,user,tick) key-collect + HashMap lookup
        // + written-kinds Vec + HashMap-remove dance of the old HashMap path.
        let mut written_count = 0usize;

        for (component_kind, kind_bit, plan_diff_mask) in kinds.iter() {
            let component_kind = *component_kind;
            let kind_bit = *kind_bit;
            // MISSION_TICK_FLOOR Lever 3: on the SERVER (entity_idx valid) the
            // per-property `DiffMask` is the FROZEN value captured at the freeze
            // point by `SendState::prepare_send_job` — NOT a live fetch. That is
            // the crux of the pure send-job: the lagged transmit reads zero live
            // per-user diff state (pre-Lever-3 this re-fetched `get_diff_mask_dense`
            // here, which desynced under the send lag). The CLIENT send is
            // synchronous (no lag), keeps the GlobalEntity-keyed live fetch, and
            // carries a placeholder mask in the plan.
            let diff_mask = if entity_idx.is_valid() {
                plan_diff_mask.clone()
            } else {
                world_manager.get_diff_mask(global_entity, &component_kind)
            };

            // When `global_diff_handler` is `Some` (server path), attempt PATH A or PATH B.
            // When `None` (client path or fallback), `optimized_write` stays `false` and
            // we fall straight through to the existing two-pass (counter + writer) path —
            // identical to the current client behavior, zero overhead.
            let mut optimized_write = false;

            if let Some(gdh) = global_diff_handler {
                let is_user_dep = gdh
                    .is_component_user_dependent(entity_idx, kind_bit)
                    .unwrap_or_else(|| component_kinds.is_user_dependent(&component_kind));
                if !is_user_dep {
                    // ── PATH A: UserIndependent ─────────────────────────────────
                    // Bytes are identical for all users with the same DiffMask.
                    // Cache hit: replay stored bytes, zero ECS reads.
                    // Cache miss: one ECS read, one serialize, store for future users/ticks.
                    if let Some(diff_mask_key) = diff_mask.as_key() {
                        let cached: CachedComponentUpdate =
                            match gdh.get_wire_cache(entity_idx, kind_bit, diff_mask_key) {
                                Some(c) => {
                                    #[cfg(feature = "bench_instrumentation")]
                                    bench_write_counters::N_PATH_A_CACHE_HITS
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    c
                                }
                                None => {
                                    #[cfg(feature = "bench_instrumentation")]
                                    bench_write_counters::N_PATH_A_CACHE_MISSES
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    // Same freeze→transmit window as the entity check in
                                    // `write_updates`, one level finer: the entity is
                                    // still alive but THIS component was removed. The
                                    // planned update has nothing left to serialize, so
                                    // drop it rather than panicking.
                                    let Some(component) =
                                        world.component_of_kind(world_entity, &component_kind)
                                    else {
                                        written_count += 1;
                                        continue;
                                    };
                                    let mut converter =
                                        world_manager.entity_converter_mut(global_world_manager);
                                    let mut temp = BitWriter::new();
                                    true.ser(&mut temp);
                                    component_kind.ser(component_kinds, &mut temp);
                                    component.write_update(&diff_mask, &mut temp, &mut converter);
                                    let c = CachedComponentUpdate::capture(&temp).expect(
                                        "component exceeds the CachedComponentUpdate \
                                         ceiling; impossible after registration check \
                                         unless max_bit_length() returned the sentinel",
                                    );
                                    gdh.set_wire_cache(entity_idx, kind_bit, diff_mask_key, c);
                                    c
                                }
                            };

                        let mut counter = writer.counter();
                        counter.count_bits(cached.bit_count);
                        if counter.overflowed() {
                            if !*has_written {
                                Self::warn_overflow_update(
                                    component_kinds.kind_to_name(&component_kind),
                                    cached.bit_count,
                                    writer.bits_free(),
                                );
                            }
                            break;
                        }

                        *has_written = true;
                        writer.append_cached_update(&cached);
                        optimized_write = true;
                    }
                    // else: diff mask > 8 bytes (unreachable for all registered components) — fall through to two-pass
                } else if let Some(sm) = snapshot_map {
                    // ── PATH B: UserDependent ───────────────────────────────────
                    // EntityProperty fields resolve per-user local entity IDs — bytes differ per user.
                    // ECS is read once per component per tick into snapshot_map; all users
                    // serialize from the snapshot, never from ECS directly.
                    // Phase 1+2 guarantees every entry is present. If somehow missing,
                    // optimized_write stays false and the two-pass path below handles it.
                    if let Some(snapshot_entry) = sm.get(&(*global_entity, component_kind)) {
                        let snapshot: &dyn Replicate = snapshot_entry.as_ref();

                        let mut converter =
                            world_manager.entity_converter_mut(global_world_manager);

                        // Counter pass
                        let mut counter = writer.counter();
                        true.ser(&mut counter);
                        component_kind.ser(component_kinds, &mut counter);
                        snapshot.write_update(&diff_mask, &mut counter, &mut converter);
                        if counter.overflowed() {
                            if !*has_written {
                                Self::warn_overflow_update(
                                    component_kinds.kind_to_name(&component_kind),
                                    counter.bits_needed(),
                                    writer.bits_free(),
                                );
                            }
                            break;
                        }

                        *has_written = true;

                        // Writer pass
                        true.ser(writer);
                        component_kind.ser(component_kinds, writer);
                        snapshot.write_update(&diff_mask, writer, &mut converter);
                        optimized_write = true;
                    }
                }
                // else: UserDependent but snapshot_map is None — fall through to two-pass
            }

            if !optimized_write {
                // Old two-pass path: used by the client (global_diff_handler = None) and as
                // fallback for cases not handled by PATH A or PATH B above.
                // See the cache-miss arm above: a component removed inside the
                // freeze→transmit window has a stale plan entry, not an error.
                if !world.has_component_of_kind(world_entity, &component_kind) {
                    written_count += 1;
                    continue;
                }
                let mut converter = world_manager.entity_converter_mut(global_world_manager);
                let mut counter = writer.counter();
                true.ser(&mut counter);
                component_kind.ser(component_kinds, &mut counter);
                world
                    .component_of_kind(world_entity, &component_kind)
                    .expect("Component does not exist in World")
                    .write_update(&diff_mask, &mut counter, &mut converter);
                if counter.overflowed() {
                    if !*has_written {
                        let component_name = component_kinds.kind_to_name(&component_kind);
                        Self::warn_overflow_update(
                            component_name,
                            counter.bits_needed(),
                            writer.bits_free(),
                        );
                    }
                    break;
                }
                *has_written = true;
                true.ser(writer);
                component_kind.ser(component_kinds, writer);
                world
                    .component_of_kind(world_entity, &component_kind)
                    .expect("Component does not exist in World")
                    .write_update(&diff_mask, writer, &mut converter);
            }

            written_count += 1;
            // MISSION_TICK_FLOOR Lever 3: on the server the live per-user mask was
            // already cleared in `prepare_send_job` (at the freeze point), so here
            // we ONLY record the per-packet `sent_updates` ledger (needed for the
            // NACK-driven replay; the packet_index only exists now). The client
            // (entity_idx INVALID) is synchronous and still records+clears.
            if entity_idx.is_valid() {
                world_manager.record_sent_update(
                    now,
                    packet_index,
                    global_entity,
                    &component_kind,
                    diff_mask,
                );
            } else {
                world_manager.record_update(
                    now,
                    packet_index,
                    global_entity,
                    &component_kind,
                    diff_mask,
                );
            }
        }

        // Drain the successfully-written prefix. Remaining entries (overflow case)
        // stay for the next build_one_packet call.
        kinds.drain(..written_count);
    }

    fn warn_overflow_update(component_name: String, bits_needed: u32, bits_free: u32) {
        panic!(
            "Packet Write Error: Blocking overflow detected! Data update of Component `{component_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommended to slim down this Component"
        )
    }
}

/// Coverage for the authority-axis freeze->transmit guard in [`WorldWriter::write_updates`].
///
/// This lives here as a unit test rather than in the integration harness on
/// purpose. The ordering the guard defends against -- a dirty update queued
/// while this host held authority, transmitted after that authority was lost --
/// cannot be produced through the harness's network levers: an update and the
/// authority handshake travel the same reliable ordered channel, so every lever
/// that keeps an update unacked (loss or latency client->server) also stalls the
/// authority change that is supposed to overtake it. Driving `write_updates`
/// directly is the only way to place the two events in the order that production
/// actually produces.
#[cfg(test)]
mod delegated_send_guard_tests {
    use std::{
        collections::HashMap,
        net::SocketAddr,
        sync::{Arc, RwLock},
    };

    use super::*;
    use crate::{
        world::{
            component::property::Property,
            delegation::{
                auth_channel::{EntityAuthAccessor, EntityAuthChannel},
                entity_auth_status::EntityAuthStatus,
            },
            update::{
                global_dirty_bitset::GlobalDirtyBitset,
                mut_channel::{MutChannelType, MutReceiver},
            },
        },
        BigMapKey, BitReader, ComponentKinds, EntityDoesNotExistError, HostEntityAuthStatus,
        HostType, InScopeEntities, PropertyMutator, ReplicaDynRefTrait, ReplicaDynRefWrapper,
        ReplicaRefWrapper, Replicate, ReplicatedComponent,
    };

    #[derive(Replicate)]
    struct Ghost {
        value: Property<u8>,
    }

    struct TestMutChannel {
        diff_mask_length: u8,
        receivers: Vec<MutReceiver>,
        receiver_index: HashMap<SocketAddr, usize>,
    }

    impl MutChannelType for TestMutChannel {
        fn new_receiver(&mut self, address_opt: &Option<SocketAddr>) -> Option<MutReceiver> {
            let address = (*address_opt)?;
            if let Some(index) = self.receiver_index.get(&address) {
                return Some(self.receivers[*index].clone());
            }
            let receiver = MutReceiver::new(self.diff_mask_length);
            self.receiver_index.insert(address, self.receivers.len());
            self.receivers.push(receiver.clone());
            Some(receiver)
        }

        fn send(&self, property_index: u8) {
            for receiver in &self.receivers {
                receiver.mutate(property_index);
            }
        }
    }

    /// A `GlobalWorldManagerType` whose only interesting behaviour is the
    /// authority status it reports for the entity under test.
    struct AuthGwm {
        auth: EntityAuthAccessor,
        global_dirty: Arc<GlobalDirtyBitset>,
    }

    impl InScopeEntities<GlobalEntity> for AuthGwm {
        fn has_entity(&self, _: &GlobalEntity) -> bool {
            true
        }
    }

    impl GlobalWorldManagerType for AuthGwm {
        fn component_kinds(&self, _: &GlobalEntity) -> Option<Vec<ComponentKind>> {
            Some(vec![ComponentKind::of::<Ghost>()])
        }
        fn entity_can_relate_to_user(&self, _: &GlobalEntity, _: &u64) -> bool {
            true
        }
        fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>> {
            Arc::new(RwLock::new(TestMutChannel {
                diff_mask_length,
                receivers: Vec::new(),
                receiver_index: HashMap::new(),
            }))
        }
        fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>> {
            Arc::new(RwLock::new(GlobalDiffHandler::new()))
        }
        fn register_component(
            &self,
            _: &ComponentKinds,
            _: &GlobalEntity,
            _: &ComponentKind,
            _: u8,
        ) -> PropertyMutator {
            unreachable!("not exercised by these tests")
        }
        fn get_entity_auth_accessor(&self, _: &GlobalEntity) -> EntityAuthAccessor {
            self.auth.clone()
        }
        fn entity_auth_status(&self, _: &GlobalEntity) -> Option<HostEntityAuthStatus> {
            Some(self.auth.auth_status())
        }
        fn entity_needs_mutator_for_delegation(&self, _: &GlobalEntity) -> bool {
            false
        }
        fn entity_is_replicating(&self, _: &GlobalEntity) -> bool {
            true
        }
        fn entity_is_static(&self, _: &GlobalEntity) -> bool {
            false
        }
        fn global_dirty_bitset(&self) -> Option<Arc<GlobalDirtyBitset>> {
            Some(self.global_dirty.clone())
        }
    }

    /// Reports the entity and component as present -- so the two existing
    /// staleness guards (despawned entity / removed component) both pass and the
    /// authority guard is the only thing that can stop the entry -- but panics if
    /// the send path ever actually reaches serialization.
    struct TripwireWorld {
        /// `false` models an entity despawned in the freeze->transmit window.
        entity_present: bool,
        /// `false` models every planned component kind having been removed.
        component_present: bool,
    }

    impl TripwireWorld {
        /// The default: entity and component both present, so only the
        /// authority guard can stop an entry.
        fn intact() -> Self {
            Self {
                entity_present: true,
                component_present: true,
            }
        }
    }

    impl WorldRefType<u64> for TripwireWorld {
        fn has_entity(&self, _: &u64) -> bool {
            self.entity_present
        }
        fn entities(&self) -> Vec<u64> {
            vec![1]
        }
        fn has_component<R: ReplicatedComponent>(&self, _: &u64) -> bool {
            self.component_present
        }
        fn has_component_of_kind(&self, _: &u64, _: &ComponentKind) -> bool {
            self.component_present
        }
        fn component<'a, R: ReplicatedComponent>(
            &'a self,
            _: &u64,
        ) -> Option<ReplicaRefWrapper<'a, R>> {
            panic!("serialization must not be reached: the queued update should have been dropped");
        }
        fn component_of_kind<'a>(
            &'a self,
            _: &u64,
            _: &ComponentKind,
        ) -> Option<ReplicaDynRefWrapper<'a>> {
            panic!("serialization must not be reached: the queued update should have been dropped");
        }
    }

    /// A world that genuinely serves the component, unlike [`TripwireWorld`]
    /// (whose accessors panic to prove the guard dropped the entry). The
    /// overflow tests need serialization to actually happen.
    struct LiveWorld {
        ghost: Ghost,
    }

    struct GhostDynRef<'a> {
        inner: &'a dyn Replicate,
    }
    impl<'a> ReplicaDynRefTrait for GhostDynRef<'a> {
        fn to_dyn_ref(&self) -> &dyn Replicate {
            self.inner
        }
    }

    impl WorldRefType<u64> for LiveWorld {
        fn has_entity(&self, _: &u64) -> bool {
            true
        }
        fn entities(&self) -> Vec<u64> {
            vec![1]
        }
        fn has_component<R: ReplicatedComponent>(&self, _: &u64) -> bool {
            true
        }
        fn has_component_of_kind(&self, _: &u64, _: &ComponentKind) -> bool {
            true
        }
        fn component<'a, R: ReplicatedComponent>(
            &'a self,
            _: &u64,
        ) -> Option<ReplicaRefWrapper<'a, R>> {
            unimplemented!("the update path uses component_of_kind")
        }
        fn component_of_kind<'a>(
            &'a self,
            _: &u64,
            _: &ComponentKind,
        ) -> Option<ReplicaDynRefWrapper<'a>> {
            Some(ReplicaDynRefWrapper::new(GhostDynRef {
                inner: &self.ghost,
            }))
        }
    }

    /// What one `write_updates` pass did to the single queued update.
    struct DropOutcome {
        /// The entry's planned kinds were cleared (or the list was drained).
        dropped: bool,
        /// Anything at all was serialized into the packet.
        has_written: bool,
        /// Which guard fired, if any. This is the field that distinguishes
        /// "the update was dropped" from "the update was dropped *for the
        /// reason this test is about*" -- without it a test can be green
        /// because the entity looked despawned, never reaching the authority
        /// guard it claims to cover.
        reason: Option<UpdateDropReason>,
    }

    /// Runs one `write_updates` pass over a single queued update against
    /// `world`, for an entity whose delegated authority is `status`.
    fn run_pass(world: TripwireWorld, host: HostType, status: EntityAuthStatus) -> DropOutcome {
        drop_counters::reset();
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();

        let (mutator, accessor) = EntityAuthChannel::new_channel(host);
        mutator.set_auth_status(status);

        let gwm = AuthGwm {
            auth: accessor,
            global_dirty: Arc::new(GlobalDirtyBitset::new(64, kinds.kind_count() as usize)),
        };

        let mut local_world_manager = LocalWorldManager::new(&None, host, 0, &gwm);

        let global_entity = GlobalEntity::from_u64(1);
        // Register the entity with the host engine so the send path can resolve a
        // LocalEntity for it -- otherwise it would stop at that lookup and never
        // reach either the guard or serialization.
        local_world_manager.host_init_entity(
            &global_entity,
            vec![ComponentKind::of::<Ghost>()],
            &kinds,
            false,
        );
        let mut update_list: Vec<(GlobalEntity, GlobalEntityIndex, u64, UpdateKinds)> = vec![(
            global_entity,
            GlobalEntityIndex::from(1u32),
            1u64,
            vec![(ComponentKind::of::<Ghost>(), 0, DiffMask::new(1))],
        )];

        let mut writer = BitWriter::new();
        let mut has_written = false;

        WorldWriter::write_updates(
            &kinds,
            &Instant::now(),
            &mut writer,
            &0,
            &world,
            &gwm,
            None,
            &mut local_world_manager,
            &mut has_written,
            &mut update_list,
            None,
        );

        // The drop path clears the entry's planned kinds; the surrounding loop may
        // also drain the list entirely. Either way, "nothing left to serialize" is
        // the observable outcome.
        let dropped = update_list
            .first()
            .map(|entry| entry.3.is_empty())
            .unwrap_or(true);

        let fired: Vec<UpdateDropReason> = [
            UpdateDropReason::EntityDespawned,
            UpdateDropReason::AllKindsStale,
            UpdateDropReason::AuthorityLost,
        ]
        .into_iter()
        .filter(|reason| drop_counters::count(*reason) > 0)
        .collect();
        assert!(
            fired.len() <= 1,
            "one queued update can only be dropped once; guards fired: {fired:?}",
        );

        DropOutcome {
            dropped,
            has_written,
            reason: fired.first().copied(),
        }
    }

    /// The common case: an intact world, so the authority guard is the only one
    /// that can fire.
    fn run_with_auth(host: HostType, status: EntityAuthStatus) -> DropOutcome {
        run_pass(TripwireWorld::intact(), host, status)
    }

    /// The guard's reason for existing: a client that has lost authority since
    /// the update was queued must have that update dropped, not serialized.
    /// Without the guard, `TripwireWorld` panics inside serialization -- which is
    /// exactly what production did, in `DelegatedProperty::write`.
    #[test]
    fn a_queued_update_is_dropped_when_the_client_can_no_longer_write() {
        for status in [
            EntityAuthStatus::Available,
            EntityAuthStatus::Requested,
            EntityAuthStatus::Denied,
        ] {
            let outcome = run_with_auth(HostType::Client, status);
            assert!(
                outcome.dropped,
                "{status:?} is not writable, so the queued update must be dropped"
            );
            assert!(
                !outcome.has_written,
                "{status:?} must not contribute any payload"
            );
            assert_eq!(
                outcome.reason,
                Some(UpdateDropReason::AuthorityLost),
                "{status:?} must be dropped by the authority guard specifically; \
                 any other reason means this test never reached the state it \
                 claims to cover",
            );
        }
    }

    /// The two staleness guards, pinned the same way. Together with the test
    /// above this exercises every [`UpdateDropReason`], so the reason a given
    /// entry was dropped is an asserted fact rather than an assumption.
    #[test]
    fn the_staleness_guards_report_their_own_reasons() {
        let despawned = run_pass(
            TripwireWorld {
                entity_present: false,
                component_present: true,
            },
            HostType::Client,
            EntityAuthStatus::Granted,
        );
        assert_eq!(despawned.reason, Some(UpdateDropReason::EntityDespawned));
        assert!(despawned.dropped && !despawned.has_written);

        let stale = run_pass(
            TripwireWorld {
                entity_present: true,
                component_present: false,
            },
            HostType::Client,
            EntityAuthStatus::Granted,
        );
        assert_eq!(stale.reason, Some(UpdateDropReason::AllKindsStale));
        assert!(stale.dropped && !stale.has_written);
    }

    /// The guards are ordered, and the order is load-bearing: a despawned
    /// entity must be reported as despawned even when its authority is also
    /// gone, because `has_component_of_kind` on a despawned entity is the
    /// question the despawn guard exists to avoid asking.
    #[test]
    fn despawn_is_reported_ahead_of_the_other_reasons() {
        let outcome = run_pass(
            TripwireWorld {
                entity_present: false,
                component_present: false,
            },
            HostType::Client,
            EntityAuthStatus::Available,
        );
        assert_eq!(outcome.reason, Some(UpdateDropReason::EntityDespawned));
    }

    /// The guard must not swallow legitimate traffic: a client that still holds a
    /// writable authority status reaches serialization as before. `TripwireWorld`
    /// panicking here is the proof that it got there -- the guard did NOT drop it.
    #[test]
    #[should_panic(expected = "serialization must not be reached")]
    fn a_queued_update_still_serializes_while_the_client_can_write() {
        run_with_auth(HostType::Client, EntityAuthStatus::Granted);
    }

    /// On the server every auth status is writable, so the guard is a no-op there
    /// and must never drop a server-side update.
    #[test]
    #[should_panic(expected = "serialization must not be reached")]
    fn the_server_is_never_stopped_by_the_guard() {
        run_with_auth(HostType::Server, EntityAuthStatus::Available);
    }

    /// Drive `write_updates` with a writer too small to hold the queued update,
    /// so the overflow branch of the two-pass path is the one under test.
    ///
    /// `already_written` seeds the `has_written` flag that the overflow branch
    /// consults. Returns the writer's remaining planned kinds, so a caller can
    /// tell "spilled, still queued" from "consumed".
    fn run_overflow_pass(bit_capacity: u32, already_written: bool) -> (usize, bool, u32) {
        drop_counters::reset();
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();

        let (mutator, accessor) = EntityAuthChannel::new_channel(HostType::Server);
        mutator.set_auth_status(EntityAuthStatus::Granted);

        let gwm = AuthGwm {
            auth: accessor,
            global_dirty: Arc::new(GlobalDirtyBitset::new(64, kinds.kind_count() as usize)),
        };

        let mut local_world_manager = LocalWorldManager::new(&None, HostType::Server, 0, &gwm);

        let global_entity = GlobalEntity::from_u64(1);
        local_world_manager.host_init_entity(
            &global_entity,
            vec![ComponentKind::of::<Ghost>()],
            &kinds,
            false,
        );
        let mut update_list: Vec<(GlobalEntity, GlobalEntityIndex, u64, UpdateKinds)> = vec![(
            global_entity,
            GlobalEntityIndex::from(1u32),
            1u64,
            vec![(ComponentKind::of::<Ghost>(), 0, DiffMask::new(1))],
        )];

        let mut writer = BitWriter::with_capacity(bit_capacity);
        let mut has_written = already_written;

        WorldWriter::write_updates(
            &kinds,
            &Instant::now(),
            &mut writer,
            &0,
            &LiveWorld {
                ghost: Ghost::new_complete(7),
            },
            &gwm,
            None,
            &mut local_world_manager,
            &mut has_written,
            &mut update_list,
            None,
        );

        (
            update_list.first().map(|entry| entry.3.len()).unwrap_or(0),
            has_written,
            writer.bits_written(),
        )
    }

    /// A packet that already carries data and then runs out of room must SPILL,
    /// not panic: the update stays queued for the next `build_one_packet` call.
    ///
    /// This is the benign, extremely common case -- every full packet ends this
    /// way -- and before this test nothing exercised it. A `cargo-mutants` run
    /// with the harness integration tests in scope showed that deleting the `!`
    /// in `if !*has_written` survived the whole suite, which turns this routine
    /// spill into a production `panic!` (`warn_overflow_update` panics; it is
    /// not a log line). The empty-packet test below pins the other direction.
    ///
    /// `CAPACITY_PAST_HEADER` is load-bearing and deliberately asserted: below
    /// 12 bits the *entity header* overflows and `write_updates` breaks before
    /// `write_update` is ever called, which leaves the update queued for a
    /// completely different reason and tests nothing. The `bits_written` check
    /// is what keeps this test honest if the header encoding ever grows.
    #[test]
    fn an_overflowing_update_spills_to_the_next_packet_when_something_was_written() {
        const CAPACITY_PAST_HEADER: u32 = 12;

        let (still_queued, _, bits_written) = run_overflow_pass(CAPACITY_PAST_HEADER, true);

        assert!(
            bits_written > 1,
            "the entity header itself overflowed, so the component overflow branch \
             was never reached -- raise CAPACITY_PAST_HEADER (bits_written={bits_written})",
        );
        assert_eq!(
            still_queued, 1,
            "the update did not fit, so it must remain queued for the next packet",
        );
    }

    /// The other direction: nothing has been written yet, the packet is empty,
    /// and the update *still* does not fit. It can therefore never fit in any
    /// packet, so the writer panics with a diagnostic naming the component
    /// rather than silently spinning forever on an update it can never send.
    #[test]
    #[should_panic(expected = "Blocking overflow detected")]
    fn an_update_too_big_for_an_empty_packet_is_a_loud_failure() {
        run_overflow_pass(12, false);
    }

    /// Maps the single test entity both ways; `write_command` needs a converter
    /// separate from the (mutably borrowed) `LocalWorldManager`.
    struct OneEntityConverter {
        global_entity: GlobalEntity,
    }

    impl EntityAndGlobalEntityConverter<u64> for OneEntityConverter {
        fn global_entity_to_entity(
            &self,
            global_entity: &GlobalEntity,
        ) -> Result<u64, EntityDoesNotExistError> {
            if *global_entity == self.global_entity {
                Ok(1)
            } else {
                Err(EntityDoesNotExistError)
            }
        }
        fn entity_to_global_entity(
            &self,
            entity: &u64,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            if *entity == 1 {
                Ok(self.global_entity)
            } else {
                Err(EntityDoesNotExistError)
            }
        }
    }

    /// Drive `write_command` with a single queued `InsertComponent` and report
    /// which `EntityMessageType` actually went on the wire.
    fn run_command<W: WorldRefType<u64>>(
        world: &W,
        host_track: bool,
        make_command: impl FnOnce(GlobalEntity) -> EntityCommand,
    ) -> EntityMessageType {
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();

        let (mutator, accessor) = EntityAuthChannel::new_channel(HostType::Server);
        mutator.set_auth_status(EntityAuthStatus::Granted);
        let gwm = AuthGwm {
            auth: accessor,
            global_dirty: Arc::new(GlobalDirtyBitset::new(64, kinds.kind_count() as usize)),
        };

        let mut local_world_manager = LocalWorldManager::new(&None, HostType::Server, 0, &gwm);
        let global_entity = GlobalEntity::from_u64(1);
        if host_track {
            local_world_manager.host_init_entity(
                &global_entity,
                vec![ComponentKind::of::<Ghost>()],
                &kinds,
                false,
            );
        }

        // `record_command_written` scans the sent-packet list, so the packet
        // must be opened first -- `write_commands` does this before each command.
        local_world_manager.insert_sent_command_packet(&0, Instant::now());

        let converter = OneEntityConverter { global_entity };
        let mut next_send_commands: VecDeque<(CommandId, EntityCommand)> =
            VecDeque::from(vec![(CommandId::from(0u16), make_command(global_entity))]);

        let mut writer = BitWriter::new();
        let mut last_written_id: Option<CommandId> = None;

        WorldWriter::write_command(
            &kinds,
            world,
            &converter,
            &gwm,
            &mut local_world_manager,
            &0,
            &mut writer,
            &mut last_written_id,
            true,
            &mut next_send_commands,
        );

        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        // write_command writes the CommandId first, then the EntityMessageType.
        CommandId::de(&mut reader).expect("command id");
        EntityMessageType::de(&mut reader).expect("entity message type")
    }

    /// The happy path: the entity is host-tracked and the component really is in
    /// the world, so a real `InsertComponent` goes on the wire.
    #[test]
    fn a_present_component_is_inserted_for_real() {
        let world = LiveWorld {
            ghost: Ghost::new_complete(7),
        };
        assert_eq!(
            run_command(&world, true, |e| {
                EntityCommand::InsertComponent(e, ComponentKind::of::<Ghost>())
            }),
            EntityMessageType::InsertComponent,
        );
    }

    /// The despawn-race case: the entity is no longer host-tracked, so the
    /// insert degrades to a quiet terminal `Noop` rather than serializing a
    /// component for an entity this peer does not know about.
    ///
    /// Note the asymmetry this test encodes: the *other* way to reach `Noop` --
    /// host-tracked but component missing -- is a needed-set under-supply that
    /// trips a `debug_assert!` on purpose, so it is not a reachable branch in a
    /// debug build and cannot be asserted on here.
    ///
    /// Both directions are needed: `cargo-mutants` showed that flipping
    /// `if !insert_present` survived the whole suite with the harness
    /// integration tests in scope, because nothing ever asserted on which of
    /// these two messages was written.
    #[test]
    fn an_insert_for_an_untracked_entity_degrades_to_a_noop() {
        let world = LiveWorld {
            ghost: Ghost::new_complete(7),
        };
        assert_eq!(
            run_command(&world, false, |e| {
                EntityCommand::InsertComponent(e, ComponentKind::of::<Ghost>())
            }),
            EntityMessageType::Noop,
        );
    }

    /// The RemoveComponent twin of the insert gate: an entity this peer no
    /// longer tracks must produce a `Noop`, not a remove naming a local entity
    /// that was never resolved. `cargo-mutants` showed the `!` on this gate
    /// survived the suite.
    #[test]
    fn a_remove_for_an_untracked_entity_degrades_to_a_noop() {
        let world = LiveWorld {
            ghost: Ghost::new_complete(7),
        };
        assert_eq!(
            run_command(&world, false, |e| {
                EntityCommand::RemoveComponent(e, ComponentKind::of::<Ghost>())
            }),
            EntityMessageType::Noop,
        );
    }

    /// ...and the tracked entity really does get a remove.
    #[test]
    fn a_remove_for_a_tracked_entity_is_written_for_real() {
        let world = LiveWorld {
            ghost: Ghost::new_complete(7),
        };
        assert_eq!(
            run_command(&world, true, |e| {
                EntityCommand::RemoveComponent(e, ComponentKind::of::<Ghost>())
            }),
            EntityMessageType::RemoveComponent,
        );
    }

    /// Drive `write_commands` (the command-path sibling of `write_updates`)
    /// with a writer too small for the queued command.
    fn run_command_overflow(bit_capacity: u32, already_written: bool) -> (usize, bool, u32) {
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();

        let (mutator, accessor) = EntityAuthChannel::new_channel(HostType::Server);
        mutator.set_auth_status(EntityAuthStatus::Granted);
        let gwm = AuthGwm {
            auth: accessor,
            global_dirty: Arc::new(GlobalDirtyBitset::new(64, kinds.kind_count() as usize)),
        };

        let mut local_world_manager = LocalWorldManager::new(&None, HostType::Server, 0, &gwm);
        let global_entity = GlobalEntity::from_u64(1);
        local_world_manager.host_init_entity(
            &global_entity,
            vec![ComponentKind::of::<Ghost>()],
            &kinds,
            false,
        );
        local_world_manager.insert_sent_command_packet(&0, Instant::now());

        let converter = OneEntityConverter { global_entity };
        let world = LiveWorld {
            ghost: Ghost::new_complete(7),
        };
        let mut next_send_commands: VecDeque<(CommandId, EntityCommand)> = VecDeque::from(vec![(
            CommandId::from(0u16),
            EntityCommand::InsertComponent(global_entity, ComponentKind::of::<Ghost>()),
        )]);

        let mut writer = BitWriter::with_capacity(bit_capacity);
        let mut has_written = already_written;

        WorldWriter::write_commands(
            &kinds,
            &Instant::now(),
            &mut writer,
            &0,
            &world,
            &converter,
            &gwm,
            &mut local_world_manager,
            &mut has_written,
            &mut next_send_commands,
        );

        (next_send_commands.len(), has_written, writer.bits_written())
    }

    /// The command-path twin of the update-path spill test. `write_commands`
    /// has no separate entity header, so the whole command is counted in one
    /// pass and there is no capacity band to thread: anything below the 40 bits
    /// this command needs overflows cleanly.
    ///
    /// A packet that already carries data must SPILL -- the command stays
    /// queued for the next packet. `cargo-mutants` showed that deleting the `!`
    /// in `if !*has_written` here survived the suite, which turns every full
    /// packet into a `warn_overflow_command` panic.
    #[test]
    fn an_overflowing_command_spills_to_the_next_packet_when_something_was_written() {
        const TOO_SMALL: u32 = 20;

        // Proof the fixture can succeed at all: with room, the command is written
        // and leaves the queue. Without this, the assertion below would pass just
        // as well against a setup that could never write anything.
        let (queued_with_room, _, _) = run_command_overflow(64, true);
        assert_eq!(queued_with_room, 0, "fixture cannot write even with room");

        let (still_queued, _, _) = run_command_overflow(TOO_SMALL, true);
        assert_eq!(
            still_queued, 1,
            "the command did not fit, so it must remain queued for the next packet",
        );
    }

    /// The other direction: an empty packet that still cannot fit the command
    /// means it can never be sent, so this is a loud failure rather than an
    /// infinite retry.
    #[test]
    #[should_panic(expected = "Blocking overflow detected")]
    fn a_command_too_big_for_an_empty_packet_is_a_loud_failure() {
        run_command_overflow(20, false);
    }
}
