use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

use naia_shared::{
    message_list_header,
    serde::{BitReader, Serde, UnsignedVariableInteger},
    BigMap, ChannelIndex, EntityAction, EntityActionReceiver, EntityActionType, EntityHandle,
    EntityHandleConverter, MessageId, NetEntity, NetEntityHandleConverter, Protocolize, Tick,
    WorldMutType,
};

use crate::{error::NaiaClientError, event::Event};

use super::entity_record::EntityRecord;

pub struct EntityManager<P: Protocolize, E: Copy + Eq + Hash> {
    entity_records: HashMap<E, EntityRecord<P::Kind>>,
    local_to_world_entity: HashMap<NetEntity, E>,
    pub handle_entity_map: BigMap<EntityHandle, E>,
    receiver: EntityActionReceiver<NetEntity, P::Kind>,
    received_components: HashMap<(NetEntity, P::Kind), P>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Default for EntityManager<P, E> {
    fn default() -> Self {
        Self {
            entity_records: HashMap::default(),
            local_to_world_entity: HashMap::default(),
            handle_entity_map: BigMap::default(),
            receiver: EntityActionReceiver::default(),
            received_components: HashMap::default(),
        }
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityManager<P, E> {
    // Action Reader

    pub fn read_all<W: WorldMutType<P, E>, C: ChannelIndex>(
        &mut self,
        world: &mut W,
        server_tick: Tick,
        reader: &mut BitReader,
        event_stream: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        self.read_updates(world, server_tick, reader, event_stream);
        self.read_actions(world, reader, event_stream);
    }

    fn read_message_id(
        bit_reader: &mut BitReader,
        last_id_opt: &mut Option<MessageId>,
    ) -> MessageId {
        let current_id = if let Some(last_id) = last_id_opt {
            // read diff
            let id_diff = UnsignedVariableInteger::<3>::de(bit_reader).unwrap().get() as MessageId;
            last_id.wrapping_add(id_diff)
        } else {
            // read message id
            MessageId::de(bit_reader).unwrap()
        };
        *last_id_opt = Some(current_id);
        current_id
    }

    fn read_actions<W: WorldMutType<P, E>, C: ChannelIndex>(
        &mut self,
        world: &mut W,
        reader: &mut BitReader,
        event_stream: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        let mut last_read_id: Option<MessageId> = None;
        let action_count = message_list_header::read(reader);
        for _ in 0..action_count {
            self.read_action(reader, &mut last_read_id);
        }
        self.process_incoming_actions(world, event_stream);
    }

    fn read_action(&mut self, reader: &mut BitReader, last_read_id: &mut Option<MessageId>) {
        let action_id = Self::read_message_id(reader, last_read_id);

        let action_type = EntityActionType::de(reader).unwrap();

        match action_type {
            // Entity Creation
            EntityActionType::SpawnEntity => {
                // read entity
                let net_entity = NetEntity::de(reader).unwrap();

                // read components
                let components_num = UnsignedVariableInteger::<3>::de(reader).unwrap().get();
                let mut component_kinds = Vec::new();
                for _ in 0..components_num {
                    let new_component = P::read(reader, self);
                    let new_component_kind = new_component.dyn_ref().kind();
                    self.received_components
                        .insert((net_entity, new_component_kind), new_component);
                    component_kinds.push(new_component_kind);
                }

                self.receiver.buffer_action(
                    action_id,
                    EntityAction::SpawnEntity(net_entity, component_kinds),
                );
            }
            // Entity Deletion
            EntityActionType::DespawnEntity => {
                // read all data
                let net_entity = NetEntity::de(reader).unwrap();

                self.receiver
                    .buffer_action(action_id, EntityAction::DespawnEntity(net_entity));
            }
            // Add Component to Entity
            EntityActionType::InsertComponent => {
                // read all data
                let net_entity = NetEntity::de(reader).unwrap();
                let new_component = P::read(reader, self);
                let new_component_kind = new_component.dyn_ref().kind();

                self.receiver.buffer_action(
                    action_id,
                    EntityAction::InsertComponent(net_entity, new_component_kind),
                );
                self.received_components
                    .insert((net_entity, new_component_kind), new_component);
            }
            // Component Removal
            EntityActionType::RemoveComponent => {
                // read all data
                let net_entity = NetEntity::de(reader).unwrap();
                let component_kind = P::Kind::de(reader).unwrap();

                self.receiver.buffer_action(
                    action_id,
                    EntityAction::RemoveComponent(net_entity, component_kind),
                );
            }
            EntityActionType::Noop => {
                self.receiver.buffer_action(action_id, EntityAction::Noop);
            }
        }
    }

    fn process_incoming_actions<W: WorldMutType<P, E>, C: ChannelIndex>(
        &mut self,
        world: &mut W,
        event_stream: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        let incoming_actions = self.receiver.receive_actions();

        for action in incoming_actions {
            match action {
                EntityAction::SpawnEntity(net_entity, components) => {
                    //let e_u16: u16 = net_entity.into();
                    //info!("spawn entity: {}", e_u16);

                    if self.local_to_world_entity.contains_key(&net_entity) {
                        panic!("attempted to insert duplicate entity");
                    }

                    // set up entity
                    let world_entity = world.spawn_entity();
                    self.local_to_world_entity.insert(net_entity, world_entity);
                    let entity_handle = self.handle_entity_map.insert(world_entity);
                    let mut entity_record = EntityRecord::new(net_entity, entity_handle);

                    event_stream.push_back(Ok(Event::SpawnEntity(world_entity)));

                    // read component list
                    for component_kind in components {
                        let component = self
                            .received_components
                            .remove(&(net_entity, component_kind))
                            .unwrap();

                        entity_record.component_kinds.insert(component_kind);

                        component.extract_and_insert(&world_entity, world);

                        event_stream
                            .push_back(Ok(Event::InsertComponent(world_entity, component_kind)));
                    }
                    //

                    self.entity_records.insert(world_entity, entity_record);
                }
                EntityAction::DespawnEntity(net_entity) => {
                    //let e_u16: u16 = net_entity.into();
                    //info!("despawn entity: {}", e_u16);

                    if let Some(world_entity) = self.local_to_world_entity.remove(&net_entity) {
                        if self.entity_records.remove(&world_entity).is_none() {
                            panic!("despawning an uninitialized entity");
                        }

                        // Generate event for each component, handing references off just in
                        // case
                        for component_kind in world.component_kinds(&world_entity) {
                            if let Some(component) =
                                world.remove_component_of_kind(&world_entity, &component_kind)
                            {
                                event_stream
                                    .push_back(Ok(Event::RemoveComponent(world_entity, component)));
                            }
                        }

                        world.despawn_entity(&world_entity);

                        event_stream.push_back(Ok(Event::DespawnEntity(world_entity)));
                    } else {
                        panic!("received message attempting to delete nonexistent entity");
                    }
                }
                EntityAction::InsertComponent(net_entity, component_kind) => {
                    //let e_u16: u16 = net_entity.into();
                    //info!("insert component for: {}", e_u16);

                    let component = self
                        .received_components
                        .remove(&(net_entity, component_kind))
                        .unwrap();

                    if !self.local_to_world_entity.contains_key(&net_entity) {
                        panic!(
                            "attempting to add a component to nonexistent entity: {}",
                            Into::<u16>::into(net_entity)
                        );
                    } else {
                        let world_entity = self.local_to_world_entity.get(&net_entity).unwrap();

                        let entity_record = self.entity_records.get_mut(world_entity).unwrap();

                        entity_record.component_kinds.insert(component_kind);

                        component.extract_and_insert(world_entity, world);

                        event_stream
                            .push_back(Ok(Event::InsertComponent(*world_entity, component_kind)));
                    }
                }
                EntityAction::RemoveComponent(net_entity, component_kind) => {
                    //let e_u16: u16 = net_entity.into();
                    //info!("remove component for: {}", e_u16);

                    let world_entity = self
                        .local_to_world_entity
                        .get_mut(&net_entity)
                        .expect("attempting to delete component of nonexistent entity");
                    let entity_record = self
                        .entity_records
                        .get_mut(world_entity)
                        .expect("attempting to delete component of nonexistent entity");
                    if entity_record.component_kinds.remove(&component_kind) {
                        // Get component for last change
                        let component = world
                            .remove_component_of_kind(world_entity, &component_kind)
                            .expect("Component already removed?");

                        // Generate event
                        event_stream
                            .push_back(Ok(Event::RemoveComponent(*world_entity, component)));
                    } else {
                        panic!("attempting to delete nonexistent component of entity");
                    }
                }
                EntityAction::Noop => {
                    // do nothing
                }
            }
        }
    }

    fn read_updates<W: WorldMutType<P, E>, C: ChannelIndex>(
        &mut self,
        world: &mut W,
        server_tick: Tick,
        reader: &mut BitReader,
        event_stream: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        let update_count = message_list_header::read(reader);
        for _ in 0..update_count {
            self.read_update(world, server_tick, reader, event_stream);
        }
    }

    fn read_update<W: WorldMutType<P, E>, C: ChannelIndex>(
        &mut self,
        world: &mut W,
        server_tick: Tick,
        reader: &mut BitReader,
        event_stream: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        let net_entity = NetEntity::de(reader).unwrap();

        let components_number = UnsignedVariableInteger::<3>::de(reader).unwrap().get();

        for _ in 0..components_number {
            // read incoming update
            let component_update = P::read_create_update(reader);
            let component_kind = component_update.kind;

            if let Some(world_entity) = self.local_to_world_entity.get(&net_entity) {
                world.component_apply_update(self, world_entity, &component_kind, component_update);

                event_stream.push_back(Ok(Event::UpdateComponent(
                    server_tick,
                    *world_entity,
                    component_kind,
                )));
            }
        }
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityHandleConverter<E> for EntityManager<P, E> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> E {
        *self
            .handle_entity_map
            .get(entity_handle)
            .expect("entity does not exist for given handle!")
    }

    fn entity_to_handle(&self, entity: &E) -> EntityHandle {
        self.entity_records
            .get(entity)
            .expect("entity does not exist!")
            .entity_handle
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash> NetEntityHandleConverter for EntityManager<P, E> {
    fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> NetEntity {
        let entity = self
            .handle_entity_map
            .get(entity_handle)
            .expect("no entity exists for the given handle!");
        let entity_record = self.entity_records.get(entity).unwrap();
        entity_record.net_entity
    }

    fn net_entity_to_handle(&self, net_entity: &NetEntity) -> EntityHandle {
        let entity = self
            .local_to_world_entity
            .get(net_entity)
            .expect("no entity exists associated with given net entity");
        self.entity_to_handle(entity)
    }
}
