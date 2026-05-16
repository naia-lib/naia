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
    BitWrite, BitWriter, CachedComponentUpdate, ComponentKind, ComponentKinds,
    EntityAndGlobalEntityConverter, EntityCommand, EntityMessage, EntityMessageType, GlobalEntity,
    Instant, MessageIndex, PacketIndex, Replicate, Serde, WorldRefType,
};

/// Per-tick counters for the packet-write path.
/// Enabled via `bench_instrumentation`.
///
/// - `N_SCOPE_ENTRY_SPAWNS`: SpawnWithComponents commands actually written (not Noop'd) per tick.
#[cfg(feature = "bench_instrumentation")]
pub mod bench_write_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    #[doc(hidden)] pub static N_SCOPE_ENTRY_SPAWNS: AtomicU64 = AtomicU64::new(0);
    /// PATH A wire-cache hits: bytes replayed from cache, zero ECS reads.
    #[doc(hidden)] pub static N_PATH_A_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
    /// PATH A wire-cache misses: ECS read + serialize + store into cache.
    #[doc(hidden)] pub static N_PATH_A_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

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
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, HashMap<ComponentKind, u16>)>,
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

                let all_present = comp_kind_list
                    .iter()
                    .all(|k| world.has_component_of_kind(&world_entity, k));

                let has_global = world_manager.has_global_entity(global_entity);
                if !has_global || !all_present {
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
                    let mut converter =
                        world_manager.entity_converter_mut(global_world_manager);
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
                    bench_write_counters::N_SCOPE_ENTRY_SPAWNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

                if !world_manager.has_global_entity(global_entity)
                    || !world.has_component_of_kind(&world_entity, component_kind)
                {
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

                    let mut converter = world_manager.entity_converter_mut(global_world_manager);

                    // write component payload
                    world
                        .component_of_kind(&world_entity, component_kind)
                        .expect("Component does not exist in World")
                        .write(component_kinds, writer, &mut converter);

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
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, HashMap<ComponentKind, u16>)>,
        snapshot_map: Option<&SnapshotMap>,
    ) {
        let mut i = 0;
        while i < update_list.len() {
            // Copy the Copy fields before the mutable borrow of kinds
            let (global_entity, entity_idx, world_entity) = {
                let (ge, idx, we, _) = &update_list[i];
                (*ge, *idx, *we)
            };

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
        kinds: &mut HashMap<ComponentKind, u16>,
        snapshot_map: Option<&SnapshotMap>,
    ) {
        let mut written_component_kinds = Vec::new();
        let component_kind_set: Vec<ComponentKind> = kinds.keys().cloned().collect();

        for component_kind in &component_kind_set {
            let kind_bit = *kinds.get(component_kind).expect("kind_bit in update kinds map");
            // Hot path: use compact-key lookup when entity_idx is valid (server).
            // Falls back to the old GlobalEntity-keyed path on the client (entity_idx = INVALID).
            let diff_mask = if entity_idx.is_valid() {
                world_manager.get_diff_mask_dense(entity_idx, kind_bit)
                    .unwrap_or_else(|| world_manager.get_diff_mask(global_entity, component_kind))
            } else {
                world_manager.get_diff_mask(global_entity, component_kind)
            };

            // When `global_diff_handler` is `Some` (server path), attempt PATH A or PATH B.
            // When `None` (client path or fallback), `optimized_write` stays `false` and
            // we fall straight through to the existing two-pass (counter + writer) path —
            // identical to the current client behavior, zero overhead.
            let mut optimized_write = false;

            if let Some(gdh) = global_diff_handler {
                let is_user_dep = gdh
                    .is_component_user_dependent(entity_idx, kind_bit)
                    .unwrap_or_else(|| component_kinds.is_user_dependent(component_kind));
                if !is_user_dep {
                    // ── PATH A: UserIndependent ─────────────────────────────────
                    // Bytes are identical for all users with the same DiffMask.
                    // Cache hit: replay stored bytes, zero ECS reads.
                    // Cache miss: one ECS read, one serialize, store for future users/ticks.
                    if let Some(diff_mask_key) = diff_mask.as_key() {
                        let cached: CachedComponentUpdate = match gdh.get_wire_cache(entity_idx, kind_bit, diff_mask_key) {
                            Some(c) => {
                                #[cfg(feature = "bench_instrumentation")]
                                bench_write_counters::N_PATH_A_CACHE_HITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                c
                            }
                            None => {
                                #[cfg(feature = "bench_instrumentation")]
                                bench_write_counters::N_PATH_A_CACHE_MISSES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                let mut converter = world_manager.entity_converter_mut(global_world_manager);
                                let mut temp = BitWriter::new();
                                true.ser(&mut temp);
                                component_kind.ser(component_kinds, &mut temp);
                                world.component_of_kind(world_entity, component_kind)
                                    .expect("Component does not exist in World")
                                    .write_update(&diff_mask, &mut temp, &mut converter);
                                let c = CachedComponentUpdate::capture(&temp)
                                    .expect("component exceeds 512 bits; impossible after registration check");
                                gdh.set_wire_cache(entity_idx, kind_bit, diff_mask_key, c);
                                c
                            }
                        };

                        let mut counter = writer.counter();
                        counter.count_bits(cached.bit_count);
                        if counter.overflowed() {
                            if !*has_written {
                                Self::warn_overflow_update(component_kinds.kind_to_name(component_kind), cached.bit_count, writer.bits_free());
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
                    if let Some(snapshot_entry) = sm.get(&(*global_entity, *component_kind)) {
                        let snapshot: &dyn Replicate = snapshot_entry.as_ref();

                        let mut converter = world_manager.entity_converter_mut(global_world_manager);

                        // Counter pass
                        let mut counter = writer.counter();
                        true.ser(&mut counter);
                        component_kind.ser(component_kinds, &mut counter);
                        snapshot.write_update(&diff_mask, &mut counter, &mut converter);
                        if counter.overflowed() {
                            if !*has_written {
                                Self::warn_overflow_update(component_kinds.kind_to_name(component_kind), counter.bits_needed(), writer.bits_free());
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
                let mut converter = world_manager.entity_converter_mut(global_world_manager);
                let mut counter = writer.counter();
                true.ser(&mut counter);
                component_kind.ser(component_kinds, &mut counter);
                world.component_of_kind(world_entity, component_kind)
                    .expect("Component does not exist in World")
                    .write_update(&diff_mask, &mut counter, &mut converter);
                if counter.overflowed() {
                    if !*has_written {
                        let component_name = component_kinds.kind_to_name(component_kind);
                        Self::warn_overflow_update(component_name, counter.bits_needed(), writer.bits_free());
                    }
                    break;
                }
                *has_written = true;
                true.ser(writer);
                component_kind.ser(component_kinds, writer);
                world.component_of_kind(world_entity, component_kind)
                    .expect("Component does not exist in World")
                    .write_update(&diff_mask, writer, &mut converter);
            }

            written_component_kinds.push(*component_kind);
            // Hot path on server (entity_idx.is_valid()): compact-key clear_diff_mask, no RwLock.
            if entity_idx.is_valid() {
                world_manager.record_update_dense(now, packet_index, global_entity, entity_idx, component_kind, kind_bit, diff_mask);
            } else {
                world_manager.record_update(now, packet_index, global_entity, component_kind, diff_mask);
            }
        }

        for component_kind in &written_component_kinds {
            kinds.remove(component_kind);
        }
    }

    fn warn_overflow_update(component_name: String, bits_needed: u32, bits_free: u32) {
        panic!(
            "Packet Write Error: Blocking overflow detected! Data update of Component `{component_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommended to slim down this Component"
        )
    }
}
