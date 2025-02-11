use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use log::{info, warn};
use naia_socket_shared::Instant;

use crate::{world::{
    entity::local_entity::RemoteEntity,
    local_world_manager::LocalWorldManager,
    remote::{
        entity_event::EntityEvent,
        entity_waitlist::{EntityWaitlist, WaitlistHandle, WaitlistStore},
        remote_world_reader::RemoteWorldEvents,
    },
}, ComponentFieldUpdate, ComponentKind, ComponentKinds, ComponentUpdate, EntityAction, EntityAndGlobalEntityConverter, GlobalEntity, GlobalEntitySpawner, GlobalWorldManagerType, LocalEntityAndGlobalEntityConverter, Replicate, Tick, WorldMutType};

pub struct RemoteWorldManager {
    pub entity_waitlist: EntityWaitlist,
    insert_waitlist_store: WaitlistStore<(GlobalEntity, Box<dyn Replicate>)>,
    insert_waitlist_map: HashMap<(GlobalEntity, ComponentKind), WaitlistHandle>,
    update_waitlist_store: WaitlistStore<(Tick, GlobalEntity, ComponentKind, ComponentFieldUpdate)>,
    update_waitlist_map: HashMap<(GlobalEntity, ComponentKind), HashMap<u8, WaitlistHandle>>,
    outgoing_events: Vec<EntityEvent>,
}

impl RemoteWorldManager {
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

    pub fn on_entity_channel_opened(&mut self, remote_entity: &RemoteEntity) {
        self.entity_waitlist.add_entity(remote_entity);
    }

    pub fn on_entity_channel_closing(&mut self, remote_entity: &RemoteEntity) {
        self.entity_waitlist.remove_entity(remote_entity);
    }

    pub fn process_world_events<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        local_world_manager: &mut LocalWorldManager,
        component_kinds: &ComponentKinds,
        world: &mut W,
        now: &Instant,
        world_events: RemoteWorldEvents,
    ) -> Vec<EntityEvent> {
        self.process_updates(
            local_world_manager.entity_converter(),
            spawner.to_converter(),
            component_kinds,
            world,
            now,
            world_events.incoming_updates,
        );
        self.process_actions(
            spawner,
            global_world_manager,
            local_world_manager,
            world,
            now,
            world_events.incoming_actions,
            world_events.incoming_components,
        );

        std::mem::take(&mut self.outgoing_events)
    }

    /// Process incoming Entity actions.
    ///
    /// * Emits client events corresponding to any [`EntityAction`] received
    /// Store
    pub fn process_actions<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        local_world_manager: &mut LocalWorldManager,
        world: &mut W,
        now: &Instant,
        incoming_actions: Vec<EntityAction<RemoteEntity>>,
        incoming_components: HashMap<(RemoteEntity, ComponentKind), Box<dyn Replicate>>,
    ) {
        self.process_ready_actions(
            spawner,
            global_world_manager,
            local_world_manager,
            world,
            incoming_actions,
            incoming_components,
        );
        let world_converter = spawner.to_converter();
        self.process_waitlist_actions(local_world_manager.entity_converter(), world_converter, world, now);
    }

    /// For each [`EntityAction`] that can be executed now,
    /// execute it and emit a corresponding event.
    fn process_ready_actions<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        local_world_manager: &mut LocalWorldManager,
        world: &mut W,
        incoming_actions: Vec<EntityAction<RemoteEntity>>,
        mut incoming_components: HashMap<(RemoteEntity, ComponentKind), Box<dyn Replicate>>,
    ) {
        // execute the action and emit an event
        for action in incoming_actions {
            match action {
                EntityAction::SpawnEntity(remote_entity, components) => {
                    // set up entity
                    let world_entity = world.spawn_entity();
                    let global_entity = spawner.spawn(world_entity);
                    local_world_manager.insert_remote_entity(&global_entity, remote_entity);

                    self.outgoing_events.push(EntityEvent::SpawnEntity(global_entity));

                    // read component list
                    for component_kind in components {
                        let component = incoming_components
                            .remove(&(remote_entity, component_kind))
                            .unwrap();

                        self.process_insert(world, global_entity, world_entity, component, &component_kind);
                    }
                }
                EntityAction::DespawnEntity(remote_entity) => {
                    let global_entity = local_world_manager.remove_by_remote_entity(&remote_entity);
                    let world_entity = spawner.global_entity_to_entity(&global_entity).unwrap();

                    // Generate event for each component, handing references off just in
                    // case
                    if let Some(component_kinds) =
                        global_world_manager.component_kinds(&global_entity)
                    {
                        for component_kind in component_kinds {
                            self.process_remove(world, global_entity, world_entity, component_kind);
                        }
                    }

                    world.despawn_entity(&world_entity);

                    self.on_entity_channel_closing(&remote_entity);

                    self.outgoing_events
                        .push(EntityEvent::DespawnEntity(global_entity));
                }
                EntityAction::InsertComponent(remote_entity, component_kind) => {
                    let component = incoming_components
                        .remove(&(remote_entity, component_kind))
                        .unwrap();

                    if local_world_manager.has_remote_entity(&remote_entity) {
                        let global_entity =
                            local_world_manager.global_entity_from_remote(&remote_entity);
                        let world_entity = spawner.global_entity_to_entity(&global_entity).unwrap();

                        self.process_insert(world, global_entity, world_entity, component, &component_kind);
                    } else {
                        // entity may have despawned on disconnect or something similar?
                        warn!("received InsertComponent message for nonexistant entity");
                    }
                }
                EntityAction::RemoveComponent(remote_entity, component_kind) => {
                    let global_entity = local_world_manager.global_entity_from_remote(&remote_entity);
                    let world_entity = spawner.global_entity_to_entity(&global_entity).unwrap();
                    self.process_remove(world, global_entity, world_entity, component_kind);
                }
                EntityAction::Noop => {
                    // do nothing
                }
            }
        }
    }

    fn process_insert<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: GlobalEntity,
        world_entity: E,
        component: Box<dyn Replicate>,
        component_kind: &ComponentKind,
    ) {
        if let Some(entity_set) = component.relations_waiting() {
            let handle = self.entity_waitlist.queue(
                &entity_set,
                &mut self.insert_waitlist_store,
                (global_entity, component),
            );
            self.insert_waitlist_map
                .insert((global_entity, *component_kind), handle);
        } else {
            self.finish_insert(world, global_entity, world_entity, component, component_kind);
        }
    }

    fn finish_insert<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: GlobalEntity,
        world_entity: E,
        component: Box<dyn Replicate>,
        component_kind: &ComponentKind,
    ) {
        world.insert_boxed_component(&world_entity, component);

        self.outgoing_events.push(EntityEvent::InsertComponent(
            global_entity,
            *component_kind,
        ));
    }

    fn process_remove<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: GlobalEntity,
        world_entity: E,
        component_kind: ComponentKind,
    ) {
        // Remove from insert waitlist if it's there
        if let Some(handle) = self
            .insert_waitlist_map
            .remove(&(global_entity, component_kind))
        {
            self.insert_waitlist_store.remove(&handle);
            self.entity_waitlist.remove_waiting_handle(&handle);
            return;
        }
        // Remove Component from update waitlist if it's there
        if let Some(handle_map) = self
            .update_waitlist_map
            .remove(&(global_entity, component_kind))
        {
            for (_index, handle) in handle_map {
                self.update_waitlist_store.remove(&handle);
                self.entity_waitlist.remove_waiting_handle(&handle);
            }
            return;
        }
        // Remove from world
        if let Some(component) = world.remove_component_of_kind(&world_entity, &component_kind) {
            // Send out event
            self.outgoing_events
                .push(EntityEvent::RemoveComponent(global_entity, component));
        }
    }

    fn process_waitlist_actions<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        world: &mut W,
        now: &Instant,
    ) {
        if let Some(list) = self
            .entity_waitlist
            .collect_ready_items(now, &mut self.insert_waitlist_store)
        {
            for (global_entity, mut component) in list {
                let component_kind = component.kind();

                self.insert_waitlist_map
                    .remove(&(global_entity, component_kind));

                {
                    component.relations_complete(local_converter);
                }

                let world_entity = world_converter
                    .global_entity_to_entity(&global_entity)
                    .unwrap();
                self.finish_insert(world, global_entity, world_entity, component, &component_kind);
            }
        }
    }

    /// Process incoming Entity updates.
    ///
    /// * Emits client events corresponding to any [`EntityAction`] received
    /// Store
    pub fn process_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        now: &Instant,
        incoming_updates: Vec<(Tick, GlobalEntity, ComponentUpdate)>,
    ) {
        self.process_ready_updates(
            local_converter,
            world_converter,
            component_kinds,
            world,
            incoming_updates,
        );
        self.process_waitlist_updates(local_converter, world_converter, world, now);
    }

    /// Process component updates from raw bits for a given entity
    fn process_ready_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        mut incoming_updates: Vec<(Tick, GlobalEntity, ComponentUpdate)>,
    ) {
        for (tick, global_entity, component_update) in incoming_updates.drain(..) {
            let component_kind = component_update.kind;

            // split the component_update into the waiting and ready parts
            let Ok((waiting_updates_opt, ready_update_opt)) =
                component_update.split_into_waiting_and_ready(local_converter, component_kinds)
            else {
                warn!("Remote World Manager: cannot read malformed component update message");
                continue;
            };

            if waiting_updates_opt.is_some() && ready_update_opt.is_some() {
                warn!("Incoming Update split into BOTH waiting and ready parts");
            }
            if waiting_updates_opt.is_some() && ready_update_opt.is_none() {
                warn!("Incoming Update split into ONLY waiting part");
            }
            if waiting_updates_opt.is_none() && ready_update_opt.is_some() {
                // warn!("Incoming Update split into ONLY ready part");
            }
            if waiting_updates_opt.is_none() && ready_update_opt.is_none() {
                panic!("Incoming Update split into NEITHER waiting nor ready parts. This should not happen.");
            }

            // if it exists, queue the waiting part of the component update
            if let Some(waiting_updates) = waiting_updates_opt {
                for (waiting_entity, waiting_field_update) in waiting_updates {
                    let field_id = waiting_field_update.field_id();

                    // Have to convert the single waiting entity to a HashSet ..
                    // TODO: make this more efficient
                    let mut waiting_entities = HashSet::new();
                    waiting_entities.insert(waiting_entity);

                    let handle = self.entity_waitlist.queue(
                        &waiting_entities,
                        &mut self.update_waitlist_store,
                        (tick, global_entity, component_kind, waiting_field_update),
                    );
                    let component_field_key = (global_entity, component_kind);
                    if !self.update_waitlist_map.contains_key(&component_field_key) {
                        self.update_waitlist_map
                            .insert(component_field_key, HashMap::new());
                    }
                    let handle_map = self
                        .update_waitlist_map
                        .get_mut(&component_field_key)
                        .unwrap();
                    if let Some(old_handle) = handle_map.get(&field_id) {
                        self.update_waitlist_store.remove(&handle);
                        self.entity_waitlist.remove_waiting_handle(old_handle);
                    }
                    handle_map.insert(field_id, handle);
                }
            }
            // if it exists, apply the ready part of the component update
            if let Some(ready_update) = ready_update_opt {
                let world_entity = world_converter
                    .global_entity_to_entity(&global_entity)
                    .unwrap();
                if world
                    .component_apply_update(
                        local_converter,
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
                    global_entity,
                    component_kind,
                ));
            }
        }
    }

    fn process_waitlist_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        world: &mut W,
        now: &Instant,
    ) {
        if let Some(list) = self
            .entity_waitlist
            .collect_ready_items(now, &mut self.update_waitlist_store)
        {
            for (tick, global_entity, component_kind, ready_update) in list {
                info!("processing waiting update!");

                let component_key = (global_entity, component_kind);
                let mut remove_entry = false;
                if let Some(component_map) = self.update_waitlist_map.get_mut(&component_key) {
                    component_map.remove(&ready_update.field_id());
                    if component_map.is_empty() {
                        remove_entry = true;
                    }
                }
                if remove_entry {
                    self.update_waitlist_map.remove(&component_key);
                }

                let world_entity = world_converter.global_entity_to_entity(&global_entity).unwrap();

                if world
                    .component_apply_field_update(
                        local_converter,
                        &world_entity,
                        &component_kind,
                        ready_update,
                    )
                    .is_err()
                {
                    warn!("Remote World Manager: cannot read malformed complete waitlisted component update message");
                    continue;
                }

                self.outgoing_events.push(EntityEvent::UpdateComponent(
                    tick,
                    global_entity,
                    component_kind,
                ));
            }
        }
    }
}
