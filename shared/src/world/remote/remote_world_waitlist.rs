use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use log::{info, warn};

use naia_socket_shared::Instant;

use crate::{
    world::{
        entity::in_scope_entities::InScopeEntities,
        remote::remote_entity_waitlist::{RemoteEntityWaitlist, WaitlistHandle, WaitlistStore},
    },
    ComponentFieldUpdate, ComponentKind, ComponentKinds, ComponentUpdate,
    EntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverter, OwnedLocalEntity,
    RemoteEntity, Replicate, Tick, WorldMutType,
};

pub struct RemoteWorldWaitlist {
    entity_waitlist: RemoteEntityWaitlist,
    insert_waitlist_store: WaitlistStore<(RemoteEntity, Box<dyn Replicate>)>,
    insert_waitlist_map: HashMap<(RemoteEntity, ComponentKind), WaitlistHandle>,
    update_waitlist_store: WaitlistStore<(Tick, RemoteEntity, ComponentKind, ComponentFieldUpdate)>,
    update_waitlist_map: HashMap<(RemoteEntity, ComponentKind), HashMap<u8, WaitlistHandle>>,
}

impl RemoteWorldWaitlist {
    pub fn new() -> Self {
        Self {
            entity_waitlist: RemoteEntityWaitlist::new(),
            insert_waitlist_store: WaitlistStore::new(),
            insert_waitlist_map: HashMap::new(),
            update_waitlist_store: WaitlistStore::new(),
            update_waitlist_map: HashMap::new(),
        }
    }

    pub fn entity_waitlist(&self) -> &RemoteEntityWaitlist {
        &self.entity_waitlist
    }

    pub fn entity_waitlist_mut(&mut self) -> &mut RemoteEntityWaitlist {
        &mut self.entity_waitlist
    }

    pub(crate) fn waitlist_queue_entity(
        &mut self,
        in_scope_entities: &dyn InScopeEntities<RemoteEntity>,
        entity: &RemoteEntity,
        component: Box<dyn Replicate>,
        component_kind: &ComponentKind,
        entity_set: &HashSet<RemoteEntity>,
    ) {
        let handle = self.entity_waitlist.queue(
            in_scope_entities,
            entity_set,
            &mut self.insert_waitlist_store,
            (*entity, component),
        );

        self.insert_waitlist_map
            .insert((*entity, *component_kind), handle);
    }

    pub(crate) fn entities_to_insert(
        &mut self,
        now: &Instant,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Vec<(RemoteEntity, ComponentKind, Box<dyn Replicate>)> {
        let mut output = Vec::new();
        if let Some(list) = self
            .entity_waitlist
            .collect_ready_items(now, &mut self.insert_waitlist_store)
        {
            for (global_entity, mut component) in list {
                let component_kind = component.kind();

                // let name = component.name();
                // warn!(
                //     "Remote World Manager: processing waitlisted insert for component {:?} for entity {:?}",
                //     &name, global_entity
                // );

                self.insert_waitlist_map
                    .remove(&(global_entity, component_kind));

                {
                    component.relations_complete(local_converter);
                }

                output.push((global_entity, component_kind, component));
            }
        }

        output
    }

    pub fn spawn_entity(
        &mut self,
        in_scope_entities: &dyn InScopeEntities<RemoteEntity>,
        // converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &RemoteEntity,
    ) {
        self.entity_waitlist.spawn_entity(in_scope_entities, entity);
    }

    pub fn despawn_entity(&mut self, entity: &RemoteEntity) {
        self.entity_waitlist.despawn_entity(entity);
    }

    pub(crate) fn process_remove(
        &mut self,
        entity: &RemoteEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        // Remove from insert waitlist if it's there
        if let Some(handle) = self.insert_waitlist_map.remove(&(*entity, *component_kind)) {
            self.insert_waitlist_store.remove(&handle);
            self.entity_waitlist.remove_waiting_handle(&handle);
            return true;
        }
        // Remove Component from update waitlist if it's there
        if let Some(handle_map) = self.update_waitlist_map.remove(&(*entity, *component_kind)) {
            for (_index, handle) in handle_map {
                self.update_waitlist_store.remove(&handle);
                self.entity_waitlist.remove_waiting_handle(&handle);
            }
            return true;
        }
        false
    }

    /// Process component updates from raw bits for a given entity
    pub(crate) fn process_ready_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        in_scope_entities: &dyn InScopeEntities<RemoteEntity>,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        mut incoming_updates: Vec<(Tick, OwnedLocalEntity, ComponentUpdate)>,
    ) -> Vec<(Tick, OwnedLocalEntity, ComponentKind)> {
        let mut output = Vec::new();
        for (tick, local_entity, component_update) in incoming_updates.drain(..) {
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
                // Convert OwnedLocalEntity to RemoteEntity
                let OwnedLocalEntity::Remote(remote_entity) = local_entity else {
                    panic!("Expected RemoteEntity");
                };
                let remote_entity = RemoteEntity::new(remote_entity);

                for (waiting_remote_entity, waiting_field_update) in waiting_updates {
                    let field_id = waiting_field_update.field_id();

                    // Have to convert the single waiting entity to a HashSet ..
                    // TODO: make this more efficient
                    let mut waiting_entities = HashSet::new();
                    waiting_entities.insert(waiting_remote_entity);

                    let handle = self.entity_waitlist.queue(
                        in_scope_entities,
                        &waiting_entities,
                        &mut self.update_waitlist_store,
                        (tick, remote_entity, component_kind, waiting_field_update),
                    );
                    let component_field_key = (remote_entity, component_kind);
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
                let global_entity = local_converter
                    .owned_entity_to_global_entity(&local_entity)
                    .unwrap();
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

                output.push((tick, local_entity, component_kind));
            }
        }
        output
    }

    pub(crate) fn process_waitlist_updates<
        E: Copy + Eq + Hash + Send + Sync,
        W: WorldMutType<E>,
    >(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        world: &mut W,
        now: &Instant,
    ) -> Vec<(Tick, RemoteEntity, ComponentKind)> {
        let mut output = Vec::new();
        if let Some(list) = self
            .entity_waitlist
            .collect_ready_items(now, &mut self.update_waitlist_store)
        {
            for (tick, remote_entity, component_kind, ready_update) in list {
                info!("processing waiting update!");

                let component_key = (remote_entity, component_kind);
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

                let global_entity = local_converter
                    .remote_entity_to_global_entity(&remote_entity)
                    .unwrap();
                let world_entity = world_converter
                    .global_entity_to_entity(&global_entity)
                    .unwrap();

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

                output.push((tick, remote_entity, component_kind));
            }
        }

        output
    }
}
