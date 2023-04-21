use log::warn;
use std::{collections::HashMap, hash::Hash};

use crate::{
    world::{
        local_world_manager::LocalWorldManager,
        remote::{
            entity_event::EntityEvent,
            entity_waitlist::{EntityWaitlist, WaitlistHandle, WaitlistStore},
            remote_world_reader::RemoteWorldEvents,
        },
    },
    ComponentKind, ComponentKinds, ComponentUpdate, EntityAction, EntityConverter,
    GlobalWorldManagerType, LocalEntity, Replicate, Tick, WorldMutType,
};

pub struct RemoteWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    pub entity_waitlist: EntityWaitlist,
    insert_waitlist_store: WaitlistStore<(E, Box<dyn Replicate>)>,
    insert_waitlist_map: HashMap<(E, ComponentKind), WaitlistHandle>,
    update_waitlist_store: WaitlistStore<(Tick, E, ComponentUpdate)>,
    update_waitlist_map: HashMap<(E, ComponentKind), WaitlistHandle>,
    outgoing_events: Vec<EntityEvent<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> RemoteWorldManager<E> {
    pub fn new() -> Self {
        Self {
            entity_waitlist: EntityWaitlist::new(),
            insert_waitlist_store: WaitlistStore::new(),
            insert_waitlist_map: HashMap::new(),
            update_waitlist_store: WaitlistStore::new(),
            update_waitlist_map: HashMap::new(),
            outgoing_events: Vec::new(),
        }
    }

    fn on_entity_channel_opened(&mut self, local_entity: &LocalEntity) {
        self.entity_waitlist.add_entity(local_entity);
    }

    fn on_entity_channel_closing(&mut self, local_entity: &LocalEntity) {
        self.entity_waitlist.remove_entity(local_entity);
    }

    pub fn process_world_events<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        world_events: RemoteWorldEvents<E>,
    ) -> Vec<EntityEvent<E>> {
        self.process_updates(
            global_world_manager,
            local_world_manager,
            component_kinds,
            world,
            world_events.incoming_updates,
        );
        self.process_actions(
            global_world_manager,
            local_world_manager,
            world,
            world_events.incoming_actions,
            world_events.incoming_components,
        );

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
        incoming_actions: Vec<EntityAction<LocalEntity>>,
        incoming_components: HashMap<(LocalEntity, ComponentKind), Box<dyn Replicate>>,
    ) {
        self.process_ready_actions(
            global_world_manager,
            local_world_manager,
            world,
            incoming_actions,
            incoming_components,
        );
        self.process_waitlist_actions(global_world_manager, local_world_manager, world);
    }

    /// For each [`EntityAction`] that can be executed now,
    /// execute it and emit a corresponding event.
    fn process_ready_actions<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        world: &mut W,
        incoming_actions: Vec<EntityAction<LocalEntity>>,
        mut incoming_components: HashMap<(LocalEntity, ComponentKind), Box<dyn Replicate>>,
    ) {
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
                        let component = incoming_components
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
                    let component = incoming_components
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

    fn process_insert<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: E,
        component: Box<dyn Replicate>,
        component_kind: &ComponentKind,
    ) {
        if let Some(entity_set) = component.relations_waiting() {
            let handle = self.entity_waitlist.queue(
                &entity_set,
                &mut self.insert_waitlist_store,
                (world_entity, component),
            );
            self.insert_waitlist_map
                .insert((world_entity, *component_kind), handle);
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
        // Remove from insert waitlist if it's there
        if let Some(handle) = self
            .insert_waitlist_map
            .remove(&(world_entity, component_kind))
        {
            self.insert_waitlist_store.remove(&handle);
            self.entity_waitlist.remove_waiting(&handle);
            return;
        }
        // Remove from update waitlist if it's there
        if let Some(handle) = self
            .update_waitlist_map
            .remove(&(world_entity, component_kind))
        {
            self.update_waitlist_store.remove(&handle);
            self.entity_waitlist.remove_waiting(&handle);
            return;
        }
        // Remove from world
        if let Some(component) = world.remove_component_of_kind(&world_entity, &component_kind) {
            // Send out event
            self.outgoing_events
                .push(EntityEvent::<E>::RemoveComponent(world_entity, component));
        }
    }

    fn process_waitlist_actions<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &mut LocalWorldManager<E>,
        world: &mut W,
    ) {
        let converter = EntityConverter::new(
            global_world_manager.to_global_entity_converter(),
            local_world_manager,
        );

        if let Some(list) = self
            .entity_waitlist
            .collect_ready_items(&mut self.insert_waitlist_store)
        {
            for (world_entity, mut component) in list {
                let component_kind = component.kind();
                self.insert_waitlist_map
                    .remove(&(world_entity, component_kind));
                component.relations_complete(&converter);
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
        incoming_updates: Vec<(Tick, E, ComponentUpdate)>,
    ) {
        self.process_ready_updates(
            global_world_manager,
            local_world_manager,
            component_kinds,
            world,
            incoming_updates,
        );
        self.process_waitlist_updates(global_world_manager, local_world_manager, world);
    }

    /// Process component updates from raw bits for a given entity
    fn process_ready_updates<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &LocalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        mut incoming_updates: Vec<(Tick, E, ComponentUpdate)>,
    ) {
        let converter = EntityConverter::new(
            global_world_manager.to_global_entity_converter(),
            local_world_manager,
        );
        for (tick, world_entity, component_update) in incoming_updates.drain(..) {
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
                let handle = self.entity_waitlist.queue(
                    &waiting_entities,
                    &mut self.update_waitlist_store,
                    (tick, world_entity, waiting_update),
                );
                self.update_waitlist_map
                    .insert((world_entity, component_kind), handle);
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
        global_world_manager: &mut dyn GlobalWorldManagerType<E>,
        local_world_manager: &LocalWorldManager<E>,
        world: &mut W,
    ) {
        let converter = EntityConverter::new(
            global_world_manager.to_global_entity_converter(),
            local_world_manager,
        );
        if let Some(list) = self
            .entity_waitlist
            .collect_ready_items(&mut self.update_waitlist_store)
        {
            for (tick, world_entity, ready_update) in list {
                let component_kind = ready_update.kind;

                self.update_waitlist_map
                    .remove(&(world_entity, component_kind));

                if world
                    .component_apply_update(
                        &converter,
                        &world_entity,
                        &component_kind,
                        ready_update,
                    )
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
