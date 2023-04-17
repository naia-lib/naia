use std::{collections::HashMap, hash::Hash};

use crate::world::local_world_manager::LocalWorldManager;
use crate::{
    messages::channels::receivers::indexed_message_reader::IndexedMessageReader,
    world::remote::entity_event::EntityEvent, BitReader, ComponentKind, ComponentKinds,
    EntityAction, EntityActionReceiver, EntityActionType, EntityAndGlobalEntityConverter,
    EntityConverter, MessageIndex, NetEntity, NetEntityAndGlobalEntityConverter, Protocol,
    Replicate, Serde, SerdeErr, Tick, UnsignedVariableInteger, WorldMutType,
};

pub struct RemoteWorldManager {
    receiver: EntityActionReceiver<NetEntity>,
    received_components: HashMap<(NetEntity, ComponentKind), Box<dyn Replicate>>,
}

impl RemoteWorldManager {
    pub fn new() -> Self {
        Self {
            receiver: EntityActionReceiver::new(),
            received_components: HashMap::default(),
        }
    }

    pub fn read_world_events<E: Copy + Eq + Hash, W: WorldMutType<E>>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        protocol: &Protocol,
        world: &mut W,
        tick: Tick,
        reader: &mut BitReader,
    ) -> Result<Vec<EntityEvent<E>>, SerdeErr> {
        let mut events = Vec::new();

        // read entity updates
        self.read_updates(
            converter,
            local_world_manager,
            &protocol.component_kinds,
            world,
            tick,
            reader,
            &mut events,
        )?;

        // read entity actions
        self.read_actions(
            converter,
            local_world_manager,
            &protocol.component_kinds,
            world,
            reader,
            &mut events,
        )?;

        Ok(events)
    }

    // Action Reader
    fn read_message_index(
        reader: &mut BitReader,
        last_index_opt: &mut Option<MessageIndex>,
    ) -> Result<MessageIndex, SerdeErr> {
        // read index
        let current_index = IndexedMessageReader::read_message_index(reader, last_index_opt)?;

        *last_index_opt = Some(current_index);

        Ok(current_index)
    }

    /// Read and process incoming actions.
    ///
    /// * Emits client events corresponding to any [`EntityAction`] received
    /// Store
    pub fn read_actions<E: Copy + Eq + Hash, W: WorldMutType<E>>(
        &mut self,
        global_entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        reader: &mut BitReader,
        events: &mut Vec<EntityEvent<E>>,
    ) -> Result<(), SerdeErr> {
        let mut last_read_id: Option<MessageIndex> = None;

        {
            let converter = EntityConverter::new(global_entity_converter, local_world_manager);
            loop {
                // read action continue bit
                let action_continue = bool::de(reader)?;
                if !action_continue {
                    break;
                }

                self.read_action(&converter, component_kinds, reader, &mut last_read_id)?;
            }
        }

        self.process_incoming_actions(local_world_manager, world, events);

        Ok(())
    }

    /// Read the bits corresponding to the EntityAction and adds the [`EntityAction`]
    /// to an internal buffer.
    ///
    /// We can use a UnorderedReliableReceiver buffer because the messages have already been
    /// ordered by the client's jitter buffer
    fn read_action(
        &mut self,
        converter: &dyn NetEntityAndGlobalEntityConverter,
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
                    let new_component = component_kinds.read(reader, converter)?;
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
                let new_component = component_kinds.read(reader, converter)?;
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
    fn process_incoming_actions<E: Copy + Eq + Hash, W: WorldMutType<E>>(
        &mut self,
        local_world_manager: &mut LocalWorldManager<E>,
        world: &mut W,
        events: &mut Vec<EntityEvent<E>>,
    ) {
        // receive the list of EntityActions that can be executed now
        let incoming_actions = self.receiver.receive_actions();

        // execute the action and emit an event
        for action in incoming_actions {
            match action {
                EntityAction::SpawnEntity(net_entity, components) => {
                    // set up entity
                    let world_entity = world.spawn_entity();
                    local_world_manager.remote_spawn_entity(&world_entity, &net_entity);

                    events.push(EntityEvent::<E>::SpawnEntity(world_entity));

                    // read component list
                    for component_kind in components {
                        let component = self
                            .received_components
                            .remove(&(net_entity, component_kind))
                            .unwrap();

                        world.insert_boxed_component(&world_entity, component);

                        events.push(EntityEvent::<E>::InsertComponent(
                            world_entity,
                            component_kind,
                        ));
                    }
                    //
                }
                EntityAction::DespawnEntity(net_entity) => {
                    let world_entity = local_world_manager.remote_despawn_entity(&net_entity);

                    // Generate event for each component, handing references off just in
                    // case
                    for component_kind in world.component_kinds(&world_entity) {
                        if let Some(component) =
                            world.remove_component_of_kind(&world_entity, &component_kind)
                        {
                            events.push(EntityEvent::<E>::RemoveComponent(world_entity, component));
                        }
                    }

                    world.despawn_entity(&world_entity);

                    events.push(EntityEvent::<E>::DespawnEntity(world_entity));
                }
                EntityAction::InsertComponent(net_entity, component_kind) => {
                    let component = self
                        .received_components
                        .remove(&(net_entity, component_kind))
                        .unwrap();

                    let world_entity = local_world_manager.get_remote_entity(&net_entity);

                    world.insert_boxed_component(&world_entity, component);

                    events.push(EntityEvent::<E>::InsertComponent(
                        world_entity,
                        component_kind,
                    ));
                }
                EntityAction::RemoveComponent(net_entity, component_kind) => {
                    let world_entity = local_world_manager.get_remote_entity(&net_entity);

                    // Get component for last change
                    let component = world
                        .remove_component_of_kind(&world_entity, &component_kind)
                        .expect("Component already removed?");

                    // Generate event
                    events.push(EntityEvent::<E>::RemoveComponent(world_entity, component));
                }
                EntityAction::Noop => {
                    // do nothing
                }
            }
        }
    }

    /// Read component updates from raw bits
    pub fn read_updates<E: Copy + Eq + Hash, W: WorldMutType<E>>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        local_world_manager: &LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        server_tick: Tick,
        reader: &mut BitReader,
        events: &mut Vec<EntityEvent<E>>,
    ) -> Result<(), SerdeErr> {
        loop {
            // read update continue bit
            let update_continue = bool::de(reader)?;
            if !update_continue {
                break;
            }

            let net_entity_id = NetEntity::de(reader)?;

            self.read_update(
                converter,
                local_world_manager,
                component_kinds,
                world,
                server_tick,
                reader,
                &net_entity_id,
                events,
            )?;
        }

        Ok(())
    }

    /// Read component updates from raw bits for a given entity
    fn read_update<E: Copy + Eq + Hash, W: WorldMutType<E>>(
        &mut self,
        global_entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        local_world_manager: &LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        server_tick: Tick,
        reader: &mut BitReader,
        net_entity_id: &NetEntity,
        events: &mut Vec<EntityEvent<E>>,
    ) -> Result<(), SerdeErr> {
        loop {
            // read update continue bit
            let component_continue = bool::de(reader)?;
            if !component_continue {
                break;
            }

            let component_update = component_kinds.read_create_update(reader)?;
            let component_kind = component_update.kind;

            let world_entity = local_world_manager.get_remote_entity(net_entity_id);
            let converter = EntityConverter::new(global_entity_converter, local_world_manager);
            world.component_apply_update(
                &converter,
                &world_entity,
                &component_kind,
                component_update,
            )?;

            events.push(EntityEvent::UpdateComponent(
                server_tick,
                world_entity,
                component_kind,
            ));
        }

        Ok(())
    }
}
