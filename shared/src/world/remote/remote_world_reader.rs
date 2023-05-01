use std::{collections::HashMap, hash::Hash};

use crate::{
    messages::channels::receivers::indexed_message_reader::IndexedMessageReader,
    world::local_world_manager::LocalWorldManager, BitReader, ComponentKind, ComponentKinds,
    ComponentUpdate, EntityAction, EntityActionReceiver, EntityActionType, EntityConverter,
    GlobalWorldManagerType, LocalEntity, LocalEntityAndGlobalEntityConverter, MessageIndex,
    Protocol, Replicate, Serde, SerdeErr, Tick, UnsignedVariableInteger,
};

pub struct RemoteWorldReader<E: Copy + Eq + Hash + Send + Sync> {
    receiver: EntityActionReceiver<LocalEntity>,
    received_components: HashMap<(LocalEntity, ComponentKind), Box<dyn Replicate>>,
    received_updates: Vec<(Tick, E, ComponentUpdate)>,
}

pub struct RemoteWorldEvents<E: Copy + Eq + Hash + Send + Sync> {
    pub incoming_actions: Vec<EntityAction<LocalEntity>>,
    pub incoming_components: HashMap<(LocalEntity, ComponentKind), Box<dyn Replicate>>,
    pub incoming_updates: Vec<(Tick, E, ComponentUpdate)>,
}

impl<E: Copy + Eq + Hash + Send + Sync> RemoteWorldReader<E> {
    pub fn new() -> Self {
        Self {
            receiver: EntityActionReceiver::new(),
            received_components: HashMap::default(),
            received_updates: Vec::new(),
        }
    }

    pub fn take_incoming_events(&mut self) -> RemoteWorldEvents<E> {
        RemoteWorldEvents {
            incoming_actions: self.receiver.receive_actions(),
            incoming_components: std::mem::take(&mut self.received_components),
            incoming_updates: std::mem::take(&mut self.received_updates),
        }
    }

    // Reading

    fn read_message_index(
        reader: &mut BitReader,
        last_index_opt: &mut Option<MessageIndex>,
    ) -> Result<MessageIndex, SerdeErr> {
        // read index
        let current_index = IndexedMessageReader::read_message_index(reader, last_index_opt)?;

        *last_index_opt = Some(current_index);

        Ok(current_index)
    }

    pub fn read_world_events(
        &mut self,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        protocol: &Protocol,
        tick: &Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // read entity updates
        self.read_updates(local_world_manager, &protocol.component_kinds, tick, reader)?;

        // read entity actions
        self.read_actions(
            global_world_manager,
            local_world_manager,
            &protocol.component_kinds,
            reader,
        )?;

        Ok(())
    }

    /// Read incoming Entity actions.
    fn read_actions(
        &mut self,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let mut last_read_id: Option<MessageIndex> = None;

        {
            let converter = EntityConverter::new(
                global_world_manager.to_global_entity_converter(),
                local_world_manager,
            );

            loop {
                // read action continue bit
                let action_continue = bool::de(reader)?;
                if !action_continue {
                    break;
                }

                self.read_action(&converter, component_kinds, reader, &mut last_read_id)?;
            }
        }

        Ok(())
    }

    /// Read the bits corresponding to the EntityAction and adds the [`EntityAction`]
    /// to an internal buffer.
    ///
    /// We can use a UnorderedReliableReceiver buffer because the messages have already been
    /// ordered by the client's jitter buffer
    fn read_action(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
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
                let local_entity = LocalEntity::remote_de(reader)?;

                // read components
                let components_num = UnsignedVariableInteger::<3>::de(reader)?.get();
                let mut component_kind_list = Vec::new();
                for _ in 0..components_num {
                    let new_component = component_kinds.read(reader, converter)?;
                    let new_component_kind = new_component.kind();
                    self.received_components
                        .insert((local_entity, new_component_kind), new_component);
                    component_kind_list.push(new_component_kind);
                }

                self.receiver.buffer_action(
                    action_id,
                    EntityAction::SpawnEntity(local_entity, component_kind_list),
                );
            }
            // Entity Deletion
            EntityActionType::DespawnEntity => {
                // read all data
                let local_entity = LocalEntity::remote_de(reader)?;

                self.receiver
                    .buffer_action(action_id, EntityAction::DespawnEntity(local_entity));
            }
            // Add Component to Entity
            EntityActionType::InsertComponent => {
                // read all data
                let local_entity = LocalEntity::remote_de(reader)?;
                let new_component = component_kinds.read(reader, converter)?;
                let new_component_kind = new_component.kind();

                self.receiver.buffer_action(
                    action_id,
                    EntityAction::InsertComponent(local_entity, new_component_kind),
                );
                self.received_components
                    .insert((local_entity, new_component_kind), new_component);
            }
            // Component Removal
            EntityActionType::RemoveComponent => {
                // read all data
                let local_entity = LocalEntity::remote_de(reader)?;
                let component_kind = ComponentKind::de(component_kinds, reader)?;

                self.receiver.buffer_action(
                    action_id,
                    EntityAction::RemoveComponent(local_entity, component_kind),
                );
            }
            EntityActionType::Noop => {
                self.receiver.buffer_action(action_id, EntityAction::Noop);
            }
        }

        Ok(())
    }

    /// Read component updates from raw bits
    fn read_updates(
        &mut self,
        local_world_manager: &LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        tick: &Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            // read update continue bit
            let update_continue = bool::de(reader)?;
            if !update_continue {
                break;
            }

            let local_entity = LocalEntity::remote_de(reader)?;

            self.read_update(
                local_world_manager,
                component_kinds,
                tick,
                reader,
                &local_entity,
            )?;
        }

        Ok(())
    }

    /// Read component updates from raw bits for a given entity
    fn read_update(
        &mut self,
        local_world_manager: &LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        tick: &Tick,
        reader: &mut BitReader,
        local_entity: &LocalEntity,
    ) -> Result<(), SerdeErr> {
        loop {
            // read update continue bit
            let component_continue = bool::de(reader)?;
            if !component_continue {
                break;
            }

            let component_update = component_kinds.read_create_update(reader)?;

            // At this point, the WorldChannel/EntityReceiver should guarantee the Entity is in scope, correct?
            if local_world_manager.has_local_entity(local_entity) {
                let world_entity = local_world_manager.get_world_entity(local_entity);

                self.received_updates
                    .push((*tick, world_entity, component_update));
            }
        }

        Ok(())
    }
}
