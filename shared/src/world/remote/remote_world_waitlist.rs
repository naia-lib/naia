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
    ComponentFieldUpdate, ComponentKind, ComponentKinds, EntityAndGlobalEntityConverter,
    LocalEntityAndGlobalEntityConverter, OwnedLocalEntity, PendingComponentUpdate, RemoteEntity,
    Replicate, Tick, WorldMutType,
};

pub struct RemoteWorldWaitlist {
    entity_waitlist: RemoteEntityWaitlist,
    insert_waitlist_store: WaitlistStore<(RemoteEntity, Box<dyn Replicate>)>,
    insert_waitlist_map: HashMap<(RemoteEntity, ComponentKind), WaitlistHandle>,
    update_waitlist_store: WaitlistStore<(Tick, RemoteEntity, ComponentKind, ComponentFieldUpdate)>,
    update_waitlist_map: HashMap<(RemoteEntity, ComponentKind), HashMap<u8, WaitlistHandle>>,
    // A component update whose OWN target entity has not yet been spawned
    // locally. Held — waiting on that entity — until it spawns, then applied in
    // tick order. This is the base case of the same causal-ordering invariant
    // the relation waitlists enforce ("apply an update only once the entities it
    // depends on exist"): an update's most fundamental dependency is the entity
    // it targets. Without this, an update that arrives before its entity's
    // spawn (send-side ordering imperfection, network reordering, or
    // loss+retransmit interleaving) would hit the `owned_entity_to_global_entity`
    // unwrap. Releasing on `spawn_entity` reuses the existing machinery.
    update_self_waitlist_store:
        WaitlistStore<(Tick, RemoteEntity, ComponentKind, PendingComponentUpdate)>,
}

impl RemoteWorldWaitlist {
    pub fn new() -> Self {
        Self {
            entity_waitlist: RemoteEntityWaitlist::new(),
            insert_waitlist_store: WaitlistStore::new(),
            insert_waitlist_map: HashMap::new(),
            update_waitlist_store: WaitlistStore::new(),
            update_waitlist_map: HashMap::new(),
            update_self_waitlist_store: WaitlistStore::new(),
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
        mut incoming_updates: Vec<(Tick, OwnedLocalEntity, PendingComponentUpdate)>,
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
                let OwnedLocalEntity::Remote { .. } = local_entity else {
                    panic!("Expected RemoteEntity");
                };
                let remote_entity = local_entity.take_remote();

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
                    self.update_waitlist_map
                        .entry(component_field_key)
                        .or_default();
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
            // The ready part has no unresolved entity-RELATION dependencies — but
            // it still depends on its OWN target entity existing. Apply it now if
            // that entity is spawned; otherwise buffer it (waiting on its own
            // entity) so it applies the moment the spawn lands, rather than
            // unwrapping a missing entity. See `update_self_waitlist_store`.
            if let Some(ready_update) = ready_update_opt {
                match local_converter.owned_entity_to_global_entity(&local_entity) {
                    Ok(global_entity) => {
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
                    Err(_) => {
                        // Target entity not spawned locally yet — defer.
                        let OwnedLocalEntity::Remote { .. } = local_entity else {
                            warn!("Remote World Manager: update for a non-remote unspawned entity; dropping");
                            continue;
                        };
                        let remote_entity = local_entity.take_remote();
                        let mut deps = HashSet::new();
                        deps.insert(remote_entity);
                        self.entity_waitlist.queue(
                            in_scope_entities,
                            &deps,
                            &mut self.update_self_waitlist_store,
                            (tick, remote_entity, component_kind, ready_update),
                        );
                    }
                }
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

    /// Flush updates that were buffered waiting on their OWN target entity to be
    /// spawned (see `update_self_waitlist_store`). Released by `spawn_entity`
    /// when that entity comes into scope. Applied in ascending tick order so a
    /// later update's value wins (each carries absolute field values).
    pub(crate) fn process_self_waitlist_updates<
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
        if let Some(mut list) = self
            .entity_waitlist
            .collect_ready_items(now, &mut self.update_self_waitlist_store)
        {
            list.sort_by_key(|(tick, _, _, _)| *tick);
            for (tick, remote_entity, component_kind, ready_update) in list {
                // The entity is now in scope (that is what released this item),
                // but guard defensively against a despawn racing the release.
                let Ok(global_entity) =
                    local_converter.remote_entity_to_global_entity(&remote_entity)
                else {
                    continue;
                };
                let Ok(world_entity) = world_converter.global_entity_to_entity(&global_entity)
                else {
                    continue;
                };
                if world
                    .component_apply_update(
                        local_converter,
                        &world_entity,
                        &component_kind,
                        ready_update,
                    )
                    .is_err()
                {
                    warn!("Remote World Manager: cannot read malformed self-waitlisted component update message");
                    continue;
                }

                output.push((tick, remote_entity, component_kind));
            }
        }

        output
    }
}

#[cfg(test)]
mod remote_world_waitlist_tests {
    //! The causal-ordering machinery: an incoming insert or update is applied
    //! only once every entity it depends on exists locally. There are three
    //! independent holding areas -- inserts waiting on a related entity,
    //! field updates waiting on a related entity, and updates waiting on their
    //! *own* target entity -- and each is released by a different call.
    //!
    //! What makes this worth testing precisely is that the failure mode of a
    //! release that fires too early is an `unwrap` on an entity that does not
    //! exist, and the failure mode of one that never fires is silence: the
    //! update is simply never applied and the two peers disagree forever.

    use std::collections::HashMap;

    use super::*;
    use crate::{
        world::{
            component::property::Property,
            test_world::{full_update, IdentityConverter, TestWorld},
        },
        BigMapKey, EntityDoesNotExistError, GlobalEntity, HostEntity,
    };

    #[derive(Replicate)]
    struct Ghost {
        value: Property<u8>,
    }

    fn kinds() -> ComponentKinds {
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();
        kinds
    }

    fn ghost() -> ComponentKind {
        ComponentKind::of::<Ghost>()
    }

    /// The set of entities the connection currently has in scope. Queuing
    /// consults it, so an entity already in scope never waits.
    #[derive(Default)]
    struct Scope(HashSet<RemoteEntity>);

    impl InScopeEntities<RemoteEntity> for Scope {
        fn has_entity(&self, entity: &RemoteEntity) -> bool {
            self.0.contains(entity)
        }
    }

    /// A local entity map whose remote->global half can be emptied, so a test
    /// can stage the exact race the release guards against: an item released
    /// by a spawn, then despawned before it is applied.
    #[derive(Default)]
    struct Map {
        remote_to_global: HashMap<RemoteEntity, GlobalEntity>,
    }

    impl Map {
        fn with(entities: &[RemoteEntity]) -> Self {
            Self {
                remote_to_global: entities
                    .iter()
                    .map(|entity| (*entity, GlobalEntity::from_u64(entity.value() as u64)))
                    .collect(),
            }
        }

        fn forget(&mut self, entity: &RemoteEntity) {
            self.remote_to_global.remove(entity);
        }
    }

    impl LocalEntityAndGlobalEntityConverter for Map {
        fn global_entity_to_host_entity(
            &self,
            _global_entity: &GlobalEntity,
        ) -> Result<HostEntity, EntityDoesNotExistError> {
            Err(EntityDoesNotExistError)
        }
        fn global_entity_to_remote_entity(
            &self,
            global_entity: &GlobalEntity,
        ) -> Result<RemoteEntity, EntityDoesNotExistError> {
            self.remote_to_global
                .iter()
                .find(|(_, mapped)| *mapped == global_entity)
                .map(|(remote, _)| *remote)
                .ok_or(EntityDoesNotExistError)
        }
        fn global_entity_to_owned_entity(
            &self,
            global_entity: &GlobalEntity,
        ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
            self.global_entity_to_remote_entity(global_entity)
                .map(|remote| remote.copy_to_owned())
        }
        fn host_entity_to_global_entity(
            &self,
            _host_entity: &HostEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            Err(EntityDoesNotExistError)
        }
        fn static_host_entity_to_global_entity(
            &self,
            _host_entity: &HostEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            Err(EntityDoesNotExistError)
        }
        fn remote_entity_to_global_entity(
            &self,
            remote_entity: &RemoteEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            self.remote_to_global
                .get(remote_entity)
                .copied()
                .ok_or(EntityDoesNotExistError)
        }
        fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
            *entity
        }
    }

    fn remote(id: u32) -> RemoteEntity {
        RemoteEntity::new(id)
    }

    /// A world already holding a remote-owned `Ghost` at `id`, ready to receive
    /// updates for it.
    fn world_holding_ghost(id: u32) -> TestWorld {
        let mut world = TestWorld::new();
        world.spawn_at(id as u64);
        world.insert_boxed_component(
            &(id as u64),
            crate::world::test_world::remote_component(&kinds(), &Ghost::new_complete(0)),
        );
        world
    }

    fn value_in(world: &TestWorld, id: u32) -> u8 {
        *world
            .value_of::<Ghost>(&(id as u64))
            .expect("the world must still hold the component")
            .value
    }

    fn an_update_setting(value: u8) -> PendingComponentUpdate {
        full_update(&kinds(), &Ghost::new_complete(value))
    }

    // -- inserts waiting on a related entity --------------------------------

    #[test]
    fn a_queued_insert_is_withheld_until_the_entity_it_waits_on_spawns() {
        let mut waitlist = RemoteWorldWaitlist::new();
        let mut scope = Scope::default();
        let target = remote(1);
        let dependency = remote(2);
        let map = Map::with(&[target, dependency]);

        waitlist.waitlist_queue_entity(
            &scope,
            &target,
            crate::world::test_world::remote_component(&kinds(), &Ghost::new_complete(5)),
            &ghost(),
            &HashSet::from([dependency]),
        );

        assert!(
            waitlist
                .entities_to_insert(&Instant::now(), &map)
                .is_empty(),
            "the insert depends on an entity that does not exist yet",
        );

        scope.0.insert(dependency);
        waitlist.spawn_entity(&scope, &dependency);

        let released = waitlist.entities_to_insert(&Instant::now(), &map);
        assert_eq!(released.len(), 1, "the spawn must release it");
        assert_eq!(released[0].0, target, "released against its own entity");
        assert_eq!(released[0].1, ghost());

        assert!(
            waitlist
                .entities_to_insert(&Instant::now(), &map)
                .is_empty(),
            "and it must be released exactly once",
        );
    }

    /// A remove arriving while the insert is still waiting must cancel it. If
    /// it did not, the insert would land *after* the remove that was meant to
    /// undo it, and the component would exist on a peer that asked for it to
    /// be gone.
    #[test]
    fn removing_a_component_cancels_the_insert_still_waiting_for_it() {
        let mut waitlist = RemoteWorldWaitlist::new();
        let mut scope = Scope::default();
        let target = remote(1);
        let dependency = remote(2);
        let map = Map::with(&[target, dependency]);

        waitlist.waitlist_queue_entity(
            &scope,
            &target,
            crate::world::test_world::remote_component(&kinds(), &Ghost::new_complete(5)),
            &ghost(),
            &HashSet::from([dependency]),
        );

        assert!(
            waitlist.process_remove(&target, &ghost()),
            "the remove must report that it cancelled something",
        );

        scope.0.insert(dependency);
        waitlist.spawn_entity(&scope, &dependency);
        assert!(
            waitlist
                .entities_to_insert(&Instant::now(), &map)
                .is_empty(),
            "the cancelled insert must not resurface when the spawn lands",
        );
    }

    #[test]
    fn removing_a_component_nothing_is_waiting_for_reports_nothing() {
        let mut waitlist = RemoteWorldWaitlist::new();
        assert!(!waitlist.process_remove(&remote(1), &ghost()));
    }

    // -- updates waiting on their own target entity -------------------------

    fn apply_ready(
        waitlist: &mut RemoteWorldWaitlist,
        scope: &Scope,
        map: &Map,
        world: &mut TestWorld,
        updates: Vec<(Tick, OwnedLocalEntity, PendingComponentUpdate)>,
    ) -> Vec<(Tick, OwnedLocalEntity, ComponentKind)> {
        waitlist.process_ready_updates(scope, map, &IdentityConverter, &kinds(), world, updates)
    }

    #[test]
    fn an_update_for_a_spawned_entity_is_applied_straight_away() {
        let mut waitlist = RemoteWorldWaitlist::new();
        let scope = Scope::default();
        let entity = remote(1);
        let map = Map::with(&[entity]);
        let mut world = world_holding_ghost(1);

        let applied = apply_ready(
            &mut waitlist,
            &scope,
            &map,
            &mut world,
            vec![(0, entity.copy_to_owned(), an_update_setting(7))],
        );

        assert_eq!(applied.len(), 1, "nothing was waiting on anything");
        assert_eq!(value_in(&world, 1), 7);
    }

    /// The base case of the whole invariant: an update whose *own* target
    /// entity has not spawned yet. Before this was buffered it hit an unwrap.
    #[test]
    fn an_update_that_outruns_its_own_entitys_spawn_is_held_not_dropped() {
        let mut waitlist = RemoteWorldWaitlist::new();
        let mut scope = Scope::default();
        let entity = remote(1);
        let mut world = TestWorld::new();

        // The entity is not in the map yet -- its spawn has not been processed.
        let applied = apply_ready(
            &mut waitlist,
            &scope,
            &Map::default(),
            &mut world,
            vec![(0, entity.copy_to_owned(), an_update_setting(7))],
        );
        assert!(applied.is_empty(), "nothing can be applied yet");

        // Now the spawn lands.
        let map = Map::with(&[entity]);
        let mut world = world_holding_ghost(1);
        scope.0.insert(entity);
        waitlist.spawn_entity(&scope, &entity);

        let flushed = waitlist.process_self_waitlist_updates(
            &map,
            &IdentityConverter,
            &mut world,
            &Instant::now(),
        );
        assert_eq!(
            flushed,
            vec![(0, entity, ghost())],
            "the held update must be flushed by the spawn",
        );
        assert_eq!(value_in(&world, 1), 7);
    }

    /// Held updates carry absolute field values, so applying them out of order
    /// would leave the component showing a value the sender has already
    /// superseded -- permanently, since nothing re-sends an unchanged field.
    #[test]
    fn held_updates_are_applied_in_tick_order_so_the_latest_value_wins() {
        let mut waitlist = RemoteWorldWaitlist::new();
        let mut scope = Scope::default();
        let entity = remote(1);
        let mut empty_world = TestWorld::new();

        // Queued newest-first, to prove the flush sorts rather than preserving
        // arrival order.
        for tick in [9u16, 3, 5] {
            apply_ready(
                &mut waitlist,
                &scope,
                &Map::default(),
                &mut empty_world,
                vec![(tick, entity.copy_to_owned(), an_update_setting(tick as u8))],
            );
        }

        let map = Map::with(&[entity]);
        let mut world = world_holding_ghost(1);
        scope.0.insert(entity);
        waitlist.spawn_entity(&scope, &entity);

        let flushed = waitlist.process_self_waitlist_updates(
            &map,
            &IdentityConverter,
            &mut world,
            &Instant::now(),
        );
        assert_eq!(
            flushed.iter().map(|(tick, _, _)| *tick).collect::<Vec<_>>(),
            vec![3, 5, 9],
            "ascending tick order",
        );
        assert_eq!(
            value_in(&world, 1),
            9,
            "and so the newest tick's value is the one left standing",
        );
    }

    /// A despawn racing the release: the spawn made the item ready, but the
    /// entity was gone again by the time the flush ran. It must be dropped,
    /// not unwrapped.
    #[test]
    fn an_update_released_for_an_entity_that_has_since_despawned_is_dropped() {
        let mut waitlist = RemoteWorldWaitlist::new();
        let mut scope = Scope::default();
        let entity = remote(1);
        let mut world = TestWorld::new();

        apply_ready(
            &mut waitlist,
            &scope,
            &Map::default(),
            &mut world,
            vec![(0, entity.copy_to_owned(), an_update_setting(7))],
        );

        scope.0.insert(entity);
        waitlist.spawn_entity(&scope, &entity);

        let mut map = Map::with(&[entity]);
        map.forget(&entity);

        assert!(
            waitlist
                .process_self_waitlist_updates(
                    &map,
                    &IdentityConverter,
                    &mut world,
                    &Instant::now(),
                )
                .is_empty(),
            "the flush must survive an entity that vanished under it",
        );
    }

    /// Only remote-addressed entities can be waited on -- a host entity is this
    /// peer's own, so an update naming an unmapped one is a protocol error, not
    /// a race. It is dropped with a warning rather than buffered forever.
    #[test]
    fn an_update_for_an_unmapped_host_entity_is_dropped_rather_than_held() {
        let mut waitlist = RemoteWorldWaitlist::new();
        let scope = Scope::default();
        let mut world = TestWorld::new();

        let applied = apply_ready(
            &mut waitlist,
            &scope,
            &Map::default(),
            &mut world,
            vec![(
                0,
                OwnedLocalEntity::Host {
                    id: 1,
                    is_static: false,
                },
                an_update_setting(7),
            )],
        );
        assert!(applied.is_empty());

        // Nothing was buffered, so no later spawn can resurrect it.
        assert!(waitlist
            .process_self_waitlist_updates(
                &Map::with(&[remote(1)]),
                &IdentityConverter,
                &mut world,
                &Instant::now(),
            )
            .is_empty());
    }
}
