use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

use log::warn;

use naia_shared::{serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde}, DiffMask, EntityActionType, LocalComponentKey, Manifest, NetEntity, PacketIndex, Protocolize, Tick, WorldMutType, MTU_SIZE_BITS, write_list_header, read_list_header};

use super::{
    entity_message_sender::EntityMessageSender, entity_record::EntityRecord,
    error::NaiaClientError, event::Event,
};

pub struct EntityManager<P: Protocolize, E: Copy + Eq + Hash> {
    entity_records: HashMap<E, EntityRecord<P::Kind>>,
    local_to_world_entity: HashMap<NetEntity, E>,
    component_to_entity_map: HashMap<LocalComponentKey, E>,
    pub message_sender: EntityMessageSender<P, E>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityManager<P, E> {
    pub fn new() -> Self {
        EntityManager {
            local_to_world_entity: HashMap::new(),
            entity_records: HashMap::new(),
            component_to_entity_map: HashMap::new(),
            message_sender: EntityMessageSender::new(),
        }
    }

    // Action Reader
    pub fn read_actions<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        server_tick: Tick,
        reader: &mut BitReader,
        event_stream: &mut VecDeque<Result<Event<P, E>, NaiaClientError>>,
    ) {
        let action_count = read_list_header(reader);
        self.process_actions(world,
                             manifest,
                             server_tick,
                             reader,
                             event_stream,
                             action_count);
    }

    fn process_actions<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        server_tick: Tick,
        reader: &mut BitReader,
        event_stream: &mut VecDeque<Result<Event<P, E>, NaiaClientError>>,
        action_count: u16,
    ) {
        for _ in 0..action_count {
            let message_type = EntityActionType::de(reader).unwrap();

            match message_type {
                EntityActionType::SpawnEntity => {
                    // Entity Creation
                    let local_id = NetEntity::de(reader).unwrap();
                    let components_num = u8::de(reader).unwrap();
                    if self.local_to_world_entity.contains_key(&local_id) {
                        // its possible we received a very late duplicate message
                        warn!("attempted to insert duplicate entity");
                        // continue reading, just don't do anything with the data
                        for _ in 0..components_num {
                            let component_kind = P::Kind::de(reader).unwrap();
                            let _component_key = LocalComponentKey::de(reader).unwrap();
                            manifest.create_replica(component_kind, reader);
                        }
                    } else {
                        // set up entity
                        let world_entity = world.spawn_entity();
                        self.local_to_world_entity.insert(local_id, world_entity);
                        self.entity_records
                            .insert(world_entity, EntityRecord::new(local_id));
                        let entity_record = self.entity_records.get_mut(&world_entity).unwrap();

                        let mut component_list: Vec<P::Kind> = Vec::new();
                        for _ in 0..components_num {
                            // Component Creation //
                            let component_kind = P::Kind::de(reader).unwrap();
                            let component_key = LocalComponentKey::de(reader).unwrap();

                            let new_component = manifest.create_replica(component_kind, reader);
                            if self.component_to_entity_map.contains_key(&component_key) {
                                panic!("attempted to insert duplicate component");
                            } else {
                                {
                                    let new_component_kind = new_component.dyn_ref().kind();
                                    entity_record
                                        .insert_component(&component_key, &new_component_kind);
                                    component_list.push(new_component_kind);
                                }

                                self.component_to_entity_map
                                    .insert(component_key, world_entity);
                                new_component.extract_and_insert(&world_entity, world);
                            }
                            ////////////////////////
                        }

                        event_stream
                            .push_back(Ok(Event::SpawnEntity(world_entity, component_list)));
                        continue;
                    }
                }
                EntityActionType::DespawnEntity => {
                    // Entity Deletion
                    let local_id = NetEntity::de(reader).unwrap();
                    if let Some(world_entity) = self.local_to_world_entity.remove(&local_id) {
                        if let Some(entity_record) = self.entity_records.remove(&world_entity) {
                            // Generate event for each component, handing references off just in
                            // case
                            for component_kind in world.component_kinds(&world_entity) {
                                if let Some(component) =
                                    world.remove_component_of_kind(&world_entity, &component_kind)
                                {
                                    event_stream.push_back(Ok(Event::RemoveComponent(
                                        world_entity,
                                        component,
                                    )));
                                }
                            }

                            for component_key in entity_record.component_keys() {
                                self.component_to_entity_map.remove(&component_key);
                            }

                            world.despawn_entity(&world_entity);

                            event_stream.push_back(Ok(Event::DespawnEntity(world_entity)));
                            continue;
                        }
                    }
                    warn!("received message attempting to delete nonexistent entity");
                }
                EntityActionType::MessageEntity => {
                    let net_entity = NetEntity::de(reader).unwrap();
                    let message_kind = P::Kind::de(reader).unwrap();

                    let new_message = manifest.create_replica(message_kind, reader);

                    if !self.local_to_world_entity.contains_key(&net_entity) {
                        // received message BEFORE spawn, or AFTER despawn
                        panic!(
                            "attempting to receive message to nonexistent entity: {}",
                            Into::<u16>::into(net_entity)
                        );
                    } else {
                        let world_entity = self.local_to_world_entity.get(&net_entity).unwrap();

                        event_stream
                            .push_back(Ok(Event::MessageEntity(*world_entity, new_message)));
                    }
                }
                EntityActionType::InsertComponent => {
                    // Add Component to Entity
                    let net_entity = NetEntity::de(reader).unwrap();
                    let component_kind = P::Kind::de(reader).unwrap();
                    let component_key = LocalComponentKey::de(reader).unwrap();

                    let new_component = manifest.create_replica(component_kind, reader);
                    if self.component_to_entity_map.contains_key(&component_key) {
                        // its possible we received a very late duplicate message
                        warn!(
                            "attempting to add duplicate local component key: {}, into entity: {}",
                            Into::<u16>::into(component_key),
                            Into::<u16>::into(net_entity)
                        );
                    } else {
                        if !self.local_to_world_entity.contains_key(&net_entity) {
                            // its possible we received a very late duplicate message
                            warn!(
                                "attempting to add a component: {}, to nonexistent entity: {}",
                                Into::<u16>::into(component_key),
                                Into::<u16>::into(net_entity)
                            );
                        } else {
                            let world_entity = self.local_to_world_entity.get(&net_entity).unwrap();
                            self.component_to_entity_map
                                .insert(component_key, *world_entity);

                            let entity_record = self.entity_records.get_mut(&world_entity).unwrap();

                            entity_record.insert_component(&component_key, &component_kind);

                            new_component.extract_and_insert(world_entity, world);

                            event_stream.push_back(Ok(Event::InsertComponent(
                                *world_entity,
                                component_kind,
                            )));
                        }
                    }
                }
                EntityActionType::UpdateComponent => {
                    // Component Update
                    let component_key = LocalComponentKey::de(reader).unwrap();

                    if let Some(world_entity) = self.component_to_entity_map.get_mut(&component_key)
                    {
                        if let Some(entity_record) = self.entity_records.get(world_entity) {
                            let component_kind =
                                entity_record.kind_from_key(&component_key).unwrap();

                            let diff_mask: DiffMask = DiffMask::de(reader).unwrap();

                            // read incoming delta
                            world.component_read_partial(
                                world_entity,
                                component_kind,
                                &diff_mask,
                                reader,
                            );

                            event_stream.push_back(Ok(Event::UpdateComponent(
                                server_tick,
                                *world_entity,
                                *component_kind,
                            )));
                        }
                    }
                }
                EntityActionType::RemoveComponent => {
                    // Component Removal
                    let component_key = LocalComponentKey::de(reader).unwrap();

                    if !self.component_to_entity_map.contains_key(&component_key) {
                        // This could happen due to a duplicated unreliable message
                        // (i.e. server re-sends "remove component" message because it believes it
                        // hasn't been delivered and then it does get
                        // delivered after, but then a duplicate message gets delivered too..)
                        warn!(
                            "attempting to remove a non-existent component: {}",
                            Into::<u16>::into(component_key),
                        );
                    } else {
                        let world_entity =
                            self.component_to_entity_map.remove(&component_key).unwrap();

                        // Get entity record, remove component
                        let component_kind = self
                            .entity_records
                            .get_mut(&world_entity)
                            .expect("entity not instantiated properly? no such entity")
                            .remove_component(&component_key)
                            .expect("entity not instantiated properly? no type");

                        // Get component for last change
                        let component = world
                            .remove_component_of_kind(&world_entity, &component_kind)
                            .expect("Component already removed?");

                        // Generate event
                        event_stream.push_back(Ok(Event::RemoveComponent(world_entity, component)));
                    }
                }
            }
        }
    }

    // EntityMessagePacketWriter

    pub fn write_messages(&mut self, writer: &mut BitWriter, packet_index: PacketIndex) {
        let mut entity_messages = self.message_sender.generate_outgoing_message_list();

        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                return;
            }

            let mut counter = BitCounter::new();
            write_list_header(&mut counter, &123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                return;
            }

            // Find how many messages will fit into the packet
            for (_, client_tick, entity, message) in entity_messages.iter() {
                Self::write_message(
                    &mut counter,
                    &self.entity_records,
                    &client_tick,
                    &entity,
                    &message,
                );
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }
            }
        }

        // Write header
        write_list_header(writer, &message_count);

        // Messages
        {
            for _ in 0..message_count {
                // Pop message
                let (message_id, client_tick, entity, message) =
                    entity_messages.pop_front().unwrap();

                // Write message
                Self::write_message(
                    writer,
                    &self.entity_records,
                    &client_tick,
                    &entity,
                    &message,
                );
                self.message_sender
                    .message_written(packet_index, client_tick, message_id);
            }
        }
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<S: BitWrite>(
        writer: &mut S,
        entity_records: &HashMap<E, EntityRecord<P::Kind>>,
        client_tick: &Tick,
        world_entity: &E,
        message: &P,
    ) {
        if let Some(entity_record) = entity_records.get(world_entity) {
            // write client tick
            client_tick.ser(writer);

            // write local entity
            entity_record.entity_net_id.ser(writer);

            // write message kind
            message.dyn_ref().kind().ser(writer);

            // write payload
            message.write(writer);
        } else {
            panic!("Cannot find the entity record to serialize entity message!");
        }
    }
}
