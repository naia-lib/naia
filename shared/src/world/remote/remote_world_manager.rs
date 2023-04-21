use log::warn;
use std::{collections::HashMap, hash::Hash};

use crate::{
    messages::channels::receivers::indexed_message_reader::IndexedMessageReader,
    world::{
        local_world_manager::LocalWorldManager,
        remote::{
            entity_event::EntityEvent,
            entity_waitlist::{EntityWaitlist, WaitlistStore, WaitlistHandle},
        },
    },
    BitReader, ComponentKind, ComponentKinds, ComponentUpdate, EntityAction, EntityActionReceiver,
    EntityActionType, EntityConverter, GlobalWorldManagerType, LocalEntity,
    LocalEntityAndGlobalEntityConverter, MessageIndex, Protocol, Replicate, Serde, SerdeErr, Tick,
    UnsignedVariableInteger, WorldMutType,
};

pub struct RemoteWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    receiver: EntityActionReceiver<LocalEntity>,
    received_components: HashMap<(LocalEntity, ComponentKind), Box<dyn Replicate>>,
    pub entity_waitlist: EntityWaitlist,
    insert_waitlist_store: WaitlistStore<(E, Box<dyn Replicate>)>,
    insert_waitlist_map: HashMap<(E, ComponentKind), WaitlistHandle>,
    update_waitlist_store: WaitlistStore<(Tick, E, ComponentUpdate)>,
    outgoing_events: Vec<EntityEvent<E>>,
    received_updates: Vec<(Tick, E, ComponentUpdate)>,
}

impl<E: Copy + Eq + Hash + Send + Sync> RemoteWorldManager<E> {
    pub fn new() -> Self {
        Self {
            receiver: EntityActionReceiver::new(),
            received_components: HashMap::default(),
            entity_waitlist: EntityWaitlist::new(),
            insert_waitlist_store: WaitlistStore::new(),
            insert_waitlist_map: HashMap::new(),
            update_waitlist_store: WaitlistStore::new(),
            outgoing_events: Vec::new(),
            received_updates: Vec::new(),
        }
    }

    fn on_entity_channel_opened(&mut self, local_entity: &LocalEntity) {
        self.entity_waitlist.add_entity(local_entity);
    }

    fn on_entity_channel_closing(&mut self, local_entity: &LocalEntity) {
        self.entity_waitlist.remove_entity(local_entity);
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
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        protocol: &Protocol,
        tick: Tick,
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
    pub fn read_actions(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
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
    pub fn read_updates(
        &mut self,
        local_world_manager: &LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        tick: Tick,
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
        tick: Tick,
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

            let world_entity = local_world_manager.get_world_entity(local_entity);

            self.received_updates
                .push((tick, world_entity, component_update));
        }

        Ok(())
    }

    // Processing

    pub fn process_world_events<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
    ) -> Vec<EntityEvent<E>> {
        self.process_updates(
            global_world_manager,
            local_world_manager,
            component_kinds,
            world,
        );

        self.process_actions(global_world_manager, local_world_manager, world);

        std::mem::take(&mut self.outgoing_events)
    }

    /// Process incoming Entity actions.
    ///
    /// * Emits client events corresponding to any [`EntityAction`] received
    /// Store
    pub fn process_actions<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        world: &mut W,
    ) {
        self.process_ready_actions(global_world_manager, local_world_manager, world);

        {
            let converter = EntityConverter::new(
                global_world_manager.to_global_entity_converter(),
                local_world_manager,
            );
            self.process_waitlist_actions(&converter, world);
        }
    }

    /// For each [`EntityAction`] that can be executed now,
    /// execute it and emit a corresponding event.
    fn process_ready_actions<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        world: &mut W,
    ) {
        // receive the list of EntityActions that can be executed now
        let incoming_actions = self.receiver.receive_actions();

        // execute the action and emit an event
        for action in incoming_actions {
            match action {
                EntityAction::SpawnEntity(local_entity, components) => {
                    // set up entity
                    let world_entity = world.spawn_entity();
                    local_world_manager.remote_spawn_entity(&world_entity, &local_entity);
                    global_world_manager.remote_spawn_entity(&world_entity);
                    self.on_entity_channel_opened(&local_entity);

                    self.outgoing_events
                        .push(EntityEvent::<E>::SpawnEntity(world_entity));

                    // read component list
                    for component_kind in components {
                        let component = self
                            .received_components
                            .remove(&(local_entity, component_kind))
                            .unwrap();

                        self.process_insert(world, world_entity, component, &component_kind);
                    }
                }
                EntityAction::DespawnEntity(local_entity) => {
                    let world_entity = local_world_manager.remote_despawn_entity(&local_entity);
                    global_world_manager.remote_despawn_entity(&world_entity);

                    // Generate event for each component, handing references off just in
                    // case
                    for component_kind in world.component_kinds(&world_entity) {
                        self.process_remove(world, world_entity, component_kind);
                    }

                    world.despawn_entity(&world_entity);
                    self.on_entity_channel_closing(&local_entity);
                    self.outgoing_events
                        .push(EntityEvent::<E>::DespawnEntity(world_entity));
                }
                EntityAction::InsertComponent(local_entity, component_kind) => {
                    let component = self
                        .received_components
                        .remove(&(local_entity, component_kind))
                        .unwrap();

                    let world_entity = local_world_manager.get_world_entity(&local_entity);

                    self.process_insert(world, world_entity, component, &component_kind);
                }
                EntityAction::RemoveComponent(local_entity, component_kind) => {
                    let world_entity = local_world_manager.get_world_entity(&local_entity);
                    self.process_remove(world, world_entity, component_kind);
                }
                EntityAction::Noop => {
                    // do nothing
                }
            }
        }
    }

    fn process_insert<W: WorldMutType<E>>(&mut self, world: &mut W, world_entity: E, component: Box<dyn Replicate>, component_kind: &ComponentKind) {
        if let Some(entity_set) = component.relations_waiting() {
            let handle = self.entity_waitlist.queue(
                &entity_set,
                &mut self.insert_waitlist_store,
                (world_entity, component),
            );
            self.insert_waitlist_map.insert((world_entity, *component_kind), handle);
        } else {
            world.insert_boxed_component(&world_entity, component);

            self.outgoing_events.push(EntityEvent::<E>::InsertComponent(
                world_entity,
                *component_kind,
            ));
        }
    }

    fn process_remove<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: E,
        component_kind: ComponentKind,
    ) {
        // Remove from waitlist if it's there
        if let Some(handle) = self.insert_waitlist_map.remove(&(
            world_entity,
            component_kind,
        )) {
            self.insert_waitlist_store.remove(&handle);
            self.entity_waitlist.remove_waiting(&handle);
            return;
        }
        // Remove from world
        if let Some(component) =
            world.remove_component_of_kind(&world_entity, &component_kind)
        {
            // Send out event
            self.outgoing_events
                .push(EntityEvent::<E>::RemoveComponent(world_entity, component));
        }
    }

    fn process_waitlist_actions<W: WorldMutType<E>>(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        world: &mut W,
    ) {
        if let Some(list) = self
            .entity_waitlist
            .collect_ready_items(&mut self.insert_waitlist_store)
        {
            for (world_entity, mut component) in list {
                let component_kind = component.kind();
                self.insert_waitlist_map.remove(&(world_entity, component_kind));
                component.relations_complete(converter);
                world.insert_boxed_component(&world_entity, component);

                self.outgoing_events.push(EntityEvent::<E>::InsertComponent(
                    world_entity,
                    component_kind,
                ));
            }
        }
    }

    /// Process incoming Entity updates.
    ///
    /// * Emits client events corresponding to any [`EntityAction`] received
    /// Store
    pub fn process_updates<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
    ) {
        self.process_ready_updates(
            global_world_manager,
            local_world_manager,
            component_kinds,
            world,
        );

        {
            let converter = EntityConverter::new(
                global_world_manager.to_global_entity_converter(),
                local_world_manager,
            );
            self.process_waitlist_updates(&converter, world);
        }
    }

    /// Process component updates from raw bits for a given entity
    fn process_ready_updates<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
    ) {
        let converter = EntityConverter::new(
            global_world_manager.to_global_entity_converter(),
            local_world_manager,
        );
        for (tick, world_entity, component_update) in self.received_updates.drain(..) {
            let component_kind = component_update.kind;

            // split the component_update into the waiting and ready parts
            let Ok((waiting_update_opt, ready_update_opt)) =
                component_update.split_into_waiting_and_ready(&converter, component_kinds) else {
                warn!("Remote World Manager: cannot read malformed component update message");
                continue;
            };

            if waiting_update_opt.is_some() && ready_update_opt.is_some() {
                warn!("Incoming Update split into BOTH waiting and ready parts");
            }
            if waiting_update_opt.is_some() && ready_update_opt.is_none() {
                warn!("Incoming Update split into ONLY waiting part");
            }
            if waiting_update_opt.is_none() && ready_update_opt.is_some() {
                // warn!("Incoming Update split into ONLY ready part");
            }
            if waiting_update_opt.is_none() && ready_update_opt.is_none() {
                panic!("Incoming Update split into NEITHER waiting nor ready parts. This should not happen.");
            }

            // if it exists, queue the waiting part of the component update
            if let Some((waiting_entities, waiting_update)) = waiting_update_opt {
                self.entity_waitlist.queue(
                    &waiting_entities,
                    &mut self.update_waitlist_store,
                    (tick, world_entity, waiting_update),
                );
            }
            // if it exists, apply the ready part of the component update
            if let Some(ready_update) = ready_update_opt {
                if world
                    .component_apply_update(
                        &converter,
                        &world_entity,
                        &component_kind,
                        ready_update,
                    )
                    .is_err()
                {
                    warn!("Remote World Manager: cannot read malformed component update message");
                    continue;
                }

                self.outgoing_events.push(EntityEvent::UpdateComponent(
                    tick,
                    world_entity,
                    component_kind,
                ));
            }
        }
    }

    fn process_waitlist_updates<W: WorldMutType<E>>(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        world: &mut W,
    ) {
        if let Some(list) = self
            .entity_waitlist
            .collect_ready_items(&mut self.update_waitlist_store)
        {
            for (tick, world_entity, ready_update) in list {
                let component_kind = ready_update.kind;
                if world
                    .component_apply_update(converter, &world_entity, &component_kind, ready_update)
                    .is_err()
                {
                    warn!("Remote World Manager: cannot read malformed complete waitlisted component update message");
                    continue;
                }

                self.outgoing_events.push(EntityEvent::<E>::UpdateComponent(
                    tick,
                    world_entity,
                    component_kind,
                ));
            }
        }
    }
}
