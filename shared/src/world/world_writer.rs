use std::{
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
};

use log::debug;

use crate::world::local::local_world_manager::LocalWorldManager;
use crate::{
    messages::channels::senders::indexed_message_writer::IndexedMessageWriter,
    world::{
        entity::entity_converters::GlobalWorldManagerType, host::host_world_manager::CommandId,
    },
    BitWrite, BitWriter, ComponentKind, ComponentKinds, ConstBitLength,
    EntityAndGlobalEntityConverter, EntityCommand, EntityMessage, EntityMessageType, GlobalEntity,
    Instant, MessageIndex, PacketIndex, Serde, WorldRefType,
};

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

    pub fn write_into_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        component_kinds: &ComponentKinds,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_manager: &mut LocalWorldManager,
        has_written: &mut bool,
        world_events: &mut VecDeque<(CommandId, EntityCommand)>,
        update_events: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) {
        // write entity updates
        Self::write_updates(
            component_kinds,
            now,
            writer,
            &packet_index,
            world,
            entity_converter,
            global_world_manager,
            world_manager,
            has_written,
            update_events,
        );

        // write entity commands
        Self::write_commands(
            component_kinds,
            now,
            writer,
            &packet_index,
            world,
            entity_converter,
            global_world_manager,
            world_manager,
            has_written,
            world_events,
        );
    }

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
                let world_entity = entity_converter
                    .global_entity_to_entity(global_entity)
                    .unwrap();

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
                // this command is only ever sent by the server, regarding server-owned entities, to clients

                // get subcommand id
                let Some(sub_id) = sub_id_opt else {
                    panic!("SetAuthority command must have a CommandId");
                };

                // write message type
                EntityMessageType::SetAuthority.ser(writer);

                // write subcommand id
                sub_id.ser(writer);

                // get host entity
                let host_entity = world_manager
                    .entity_converter()
                    .global_entity_to_host_entity(global_entity)
                    .unwrap();

                // write host entity
                host_entity.ser(writer);

                // write auth status
                auth_status.ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    world_manager.record_command_written(
                        packet_index,
                        command_id,
                        EntityMessage::SetAuthority(
                            *sub_id,
                            host_entity.copy_to_owned(),
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
                debug!("Writing MigrateResponse to packet: global={:?}, old_remote={:?}, new_host={:?}", 
                    _global_entity, old_remote_entity, new_host_entity_value);

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

    fn write_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        component_kinds: &ComponentKinds,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_manager: &mut LocalWorldManager,
        has_written: &mut bool,
        next_send_updates: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) {
        let all_update_entities: Vec<GlobalEntity> = next_send_updates.keys().copied().collect();

        for global_entity in all_update_entities {
            // get LocalEntity
            let local_entity = world_manager
                .entity_converter()
                .global_entity_to_owned_entity(&global_entity)
                .unwrap();

            // get World Entity
            let world_entity = converter.global_entity_to_entity(&global_entity).unwrap();

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

            // for component_kind in next_send_updates.get(&global_entity).unwrap() {
            //     info!("Writing update for global_entity: {:?}, local_entity {:?}, component kind {:?}", global_entity, local_entity, component_kinds.kind_to_name(component_kind));
            // }

            // write Components
            Self::write_update(
                component_kinds,
                now,
                world,
                global_world_manager,
                world_manager,
                packet_index,
                writer,
                &global_entity,
                &world_entity,
                has_written,
                next_send_updates,
            );

            // write ComponentContinue finish bit, release
            writer.release_bits(1);
            false.ser(writer);
        }

        // write EntityContinue finish bit, release
        writer.release_bits(1);
        false.ser(writer);
    }

    /// For a given entity, write component value updates into a packet
    /// Only component values that changed in the internal (naia's) host world will be written
    fn write_update<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        component_kinds: &ComponentKinds,
        now: &Instant,
        world: &W,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_manager: &mut LocalWorldManager,
        packet_index: &PacketIndex,
        writer: &mut BitWriter,
        global_entity: &GlobalEntity,
        world_entity: &E,
        has_written: &mut bool,
        next_send_updates: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) {
        let mut written_component_kinds = Vec::new();
        let component_kind_set = next_send_updates.get(global_entity).unwrap();
        for component_kind in component_kind_set {
            // get diff mask
            let diff_mask = world_manager
                .get_diff_mask(global_entity, component_kind)
                .clone();

            let mut converter = world_manager.entity_converter_mut(global_world_manager);

            // check that we can write the next component update
            let mut counter = writer.counter();
            // write ComponentContinue bit
            true.ser(&mut counter);
            // write component kind
            counter.count_bits(<ComponentKind as ConstBitLength>::const_bit_length());
            // write data
            world
                .component_of_kind(&world_entity, component_kind)
                .expect("Component does not exist in World")
                .write_update(&diff_mask, &mut counter, &mut converter);
            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of component being too big
                if !*has_written {
                    let component_name = component_kinds.kind_to_name(component_kind);
                    Self::warn_overflow_update(
                        component_name,
                        counter.bits_needed(),
                        writer.bits_free(),
                    );
                }

                break;
            }

            *has_written = true;

            // write ComponentContinue bit
            true.ser(writer);
            // write component kind
            component_kind.ser(component_kinds, writer);
            // write data
            world
                .component_of_kind(world_entity, component_kind)
                .expect("Component does not exist in World")
                .write_update(&diff_mask, writer, &mut converter);

            written_component_kinds.push(*component_kind);

            world_manager.record_update(
                now,
                packet_index,
                global_entity,
                component_kind,
                diff_mask,
            );
        }

        let update_kinds = next_send_updates.get_mut(global_entity).unwrap();
        for component_kind in &written_component_kinds {
            update_kinds.remove(component_kind);
        }
        if update_kinds.is_empty() {
            next_send_updates.remove(global_entity);
        }
    }

    fn warn_overflow_update(component_name: String, bits_needed: u32, bits_free: u32) {
        panic!(
            "Packet Write Error: Blocking overflow detected! Data update of Component `{component_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommended to slim down this Component"
        )
    }
}
