use std::{collections::HashMap, hash::Hash};

use naia_shared::{
    BigMap, BitReader, ComponentKind, ComponentKinds, EntityAction, EntityActionReceiver,
    EntityActionType, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, MessageIndex,
    NetEntity, NetEntityHandleConverter, Replicate, Serde, SerdeErr, Tick, UnsignedVariableInteger,
    WorldMutType,
};

use crate::events::Events;

use super::entity_record::EntityRecord;

pub struct EntityManager<E: Copy + Eq + Hash> {
    entity_records: HashMap<E, EntityRecord>,
    local_to_world_entity: HashMap<NetEntity, E>,
    pub handle_entity_map: BigMap<EntityHandle, E>,
    receiver: EntityActionReceiver<NetEntity>,
    received_components: HashMap<(NetEntity, ComponentKind), Box<dyn Replicate>>,
}

impl<E: Copy + Eq + Hash> Default for EntityManager<E> {
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

impl<E: Copy + Eq + Hash> EntityManager<E> {
    // Action Reader
    fn read_message_index(
        reader: &mut BitReader,
        last_index_opt: &mut Option<MessageIndex>,
    ) -> Result<MessageIndex, SerdeErr> {
        let current_index = if let Some(last_index) = last_index_opt {
            // read diff
            let index_diff = UnsignedVariableInteger::<3>::de(reader)?.get() as MessageIndex;
            last_index.wrapping_add(index_diff)
        } else {
            // read message id
            MessageIndex::de(reader)?
        };
        *last_index_opt = Some(current_index);
        Ok(current_index)
    }

    /// Read and process incoming actions.
    ///
    /// * Emits client events corresponding to any [`EntityAction`] received
    /// Store
    pub fn read_actions<W: WorldMutType<E>>(
        &mut self,
        component_kinds: &ComponentKinds,
        world: &mut W,
        reader: &mut BitReader,
        incoming_events: &mut Events<E>,
    ) -> Result<(), SerdeErr> {
        let mut last_read_id: Option<MessageIndex> = None;

        loop {
            // read action continue bit
            let action_continue = bool::de(reader)?;
            if !action_continue {
                break;
            }

            self.read_action(component_kinds, reader, &mut last_read_id)?;
        }

        self.process_incoming_actions(world, incoming_events);

        Ok(())
    }

    /// Read the bits corresponding to the EntityAction and adds the [`EntityAction`]
    /// to an internal buffer.
    ///
    /// We can use a UnorderedReliableReceiver buffer because the messages have already been
    /// ordered by the client's jitter buffer
    fn read_action(
        &mut self,
        component_kinds: &ComponentKinds,
        reader: &mut BitReader,
        last_read_id: &mut Option<MessageIndex>,
    ) -> Result<(), SerdeErr> {
        let action_id = Self::read_message_index(reader, last_read_id)?;

        let action_type = EntityActionType::de(reader)?;

        match action_type {
            // Entity Creation
            EntityActionType::SpawnEntity => {
                // read entity
                let net_entity = NetEntity::de(reader)?;

                // read components
                let components_num = UnsignedVariableInteger::<3>::de(reader)?.get();
                let mut component_kind_list = Vec::new();
                for _ in 0..components_num {
                    let new_component = component_kinds.read(reader, self)?;
                    let new_component_kind = new_component.kind();
                    self.received_components
                        .insert((net_entity, new_component_kind), new_component);
                    component_kind_list.push(new_component_kind);
                }

                self.receiver.buffer_action(
                    action_id,
                    EntityAction::SpawnEntity(net_entity, component_kind_list),
                );
            }
            // Entity Deletion
            EntityActionType::DespawnEntity => {
                // read all data
                let net_entity = NetEntity::de(reader)?;

                self.receiver
                    .buffer_action(action_id, EntityAction::DespawnEntity(net_entity));
            }
            // Add Component to Entity
            EntityActionType::InsertComponent => {
                // read all data
                let net_entity = NetEntity::de(reader)?;
                let new_component = component_kinds.read(reader, self)?;
                let new_component_kind = new_component.kind();

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
                let net_entity = NetEntity::de(reader)?;
                let component_kind = ComponentKind::de(component_kinds, reader)?;

                self.receiver.buffer_action(
                    action_id,
                    EntityAction::RemoveComponent(net_entity, component_kind),
                );
            }
            EntityActionType::Noop => {
                self.receiver.buffer_action(action_id, EntityAction::Noop);
            }
        }

        Ok(())
    }

    /// For each [`EntityAction`] that can be executed now,
    /// execute it and emit a corresponding event.
    fn process_incoming_actions<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        incoming_events: &mut Events<E>,
    ) {
        // receive the list of EntityActions that can be executed now
        let incoming_actions = self.receiver.receive_actions();

        // execute the action and emit an event
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

                    incoming_events.push_spawn(world_entity);

                    // read component list
                    for component_kind in components {
                        let component = self
                            .received_components
                            .remove(&(net_entity, component_kind))
                            .unwrap();

                        entity_record.component_kinds.insert(component_kind);

                        world.insert_boxed_component(&world_entity, component);

                        incoming_events.push_insert(world_entity, component_kind);
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
                                incoming_events.push_remove(world_entity, component);
                            }
                        }

                        world.despawn_entity(&world_entity);

                        incoming_events.push_despawn(world_entity);
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

                        world.insert_boxed_component(&world_entity, component);

                        incoming_events.push_insert(*world_entity, component_kind);
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
                        incoming_events.push_remove(*world_entity, component);
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

    /// Read component updates from raw bits
    pub fn read_updates<W: WorldMutType<E>>(
        &mut self,
        component_kinds: &ComponentKinds,
        world: &mut W,
        server_tick: Tick,
        reader: &mut BitReader,
        incoming_events: &mut Events<E>,
    ) -> Result<(), SerdeErr> {
        loop {
            // read update continue bit
            let update_continue = bool::de(reader)?;
            if !update_continue {
                break;
            }

            let net_entity_id = NetEntity::de(reader)?;

            self.read_update(
                component_kinds,
                world,
                server_tick,
                reader,
                &net_entity_id,
                incoming_events,
            )?;
        }

        Ok(())
    }

    /// Read component updates from raw bits for a given entity
    fn read_update<W: WorldMutType<E>>(
        &mut self,
        component_kinds: &ComponentKinds,
        world: &mut W,
        server_tick: Tick,
        reader: &mut BitReader,
        net_entity_id: &NetEntity,
        incoming_events: &mut Events<E>,
    ) -> Result<(), SerdeErr> {
        loop {
            // read update continue bit
            let component_continue = bool::de(reader)?;
            if !component_continue {
                break;
            }

            let component_update = component_kinds.read_create_update(reader)?;
            let component_kind = component_update.kind;

            if let Some(world_entity) = self.local_to_world_entity.get(&net_entity_id) {
                world.component_apply_update(
                    self,
                    world_entity,
                    &component_kind,
                    component_update,
                )?;

                incoming_events.push_update(server_tick, *world_entity, component_kind);
            }
        }

        Ok(())
    }
}

impl<E: Copy + Eq + Hash> EntityHandleConverter<E> for EntityManager<E> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> E {
        *self
            .handle_entity_map
            .get(entity_handle)
            .expect("entity does not exist for given handle!")
    }

    fn entity_to_handle(&self, entity: &E) -> Result<EntityHandle, EntityDoesNotExistError> {
        if let Some(record) = self.entity_records.get(entity) {
            Ok(record.entity_handle)
        } else {
            Err(EntityDoesNotExistError)
        }
    }
}

impl<E: Copy + Eq + Hash> NetEntityHandleConverter for EntityManager<E> {
    fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> NetEntity {
        let entity = self
            .handle_entity_map
            .get(entity_handle)
            .expect("no entity exists for the given handle!");
        let entity_record = self.entity_records.get(entity).unwrap();
        entity_record.net_entity
    }

    fn net_entity_to_handle(
        &self,
        net_entity: &NetEntity,
    ) -> Result<EntityHandle, EntityDoesNotExistError> {
        if let Some(entity) = self.local_to_world_entity.get(net_entity) {
            self.entity_to_handle(entity)
        } else {
            Err(EntityDoesNotExistError)
        }
    }
}
