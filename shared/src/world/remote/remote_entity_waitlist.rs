use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};

use naia_socket_shared::Instant;

use crate::{world::entity::in_scope_entities::InScopeEntities, KeyGenerator, RemoteEntity};

pub type WaitlistHandle = u16;

const PER_ENTITY_WAITLIST_CAP: usize = 128;

pub struct RemoteEntityWaitlist {
    handle_store: KeyGenerator<WaitlistHandle>,
    handle_to_required_entities: HashMap<WaitlistHandle, HashSet<RemoteEntity>>,
    waiting_entity_to_handles: HashMap<RemoteEntity, VecDeque<WaitlistHandle>>,
    ready_handles: HashSet<WaitlistHandle>,
    removed_handles: HashSet<WaitlistHandle>,
    handle_ttls: VecDeque<(Instant, WaitlistHandle)>,
    handle_ttl: Duration,
}

impl RemoteEntityWaitlist {
    pub fn new() -> Self {
        Self::with_recycle_timeout(Duration::from_secs(60))
    }

    fn with_recycle_timeout(recycle_timeout: Duration) -> Self {
        Self {
            handle_to_required_entities: HashMap::new(),
            handle_store: KeyGenerator::new(recycle_timeout),
            waiting_entity_to_handles: HashMap::new(),
            ready_handles: HashSet::new(),
            removed_handles: HashSet::new(),
            handle_ttls: VecDeque::new(),
            handle_ttl: Duration::from_secs(60),
        }
    }

    fn required_entities_are_in_scope(
        &self,
        in_scope_entities: &dyn InScopeEntities<RemoteEntity>,
        entities: &HashSet<RemoteEntity>,
    ) -> bool {
        for entity in entities {
            if !in_scope_entities.has_entity(entity) {
                return false;
            }
        }
        true
    }

    pub fn queue<T>(
        &mut self,
        in_scope_entities: &dyn InScopeEntities<RemoteEntity>,
        entities: &HashSet<RemoteEntity>,
        waitlist_store: &mut WaitlistStore<T>,
        item: T,
    ) -> WaitlistHandle {
        let new_handle = self.handle_store.generate();

        // if all entities are in scope, we can send the message immediately
        if self.required_entities_are_in_scope(in_scope_entities, entities) {
            //info!("Entity's dependencies {:?} are in scope", entities);
            waitlist_store.queue(new_handle, item);
            self.ready_handles.insert(new_handle);
            return new_handle;
        }

        // Enforce per-entity FIFO cap: evict oldest handles before inserting.
        let mut evictions = HashSet::new();
        for entity in entities {
            if let Some(queue) = self.waiting_entity_to_handles.get_mut(entity) {
                if queue.len() >= PER_ENTITY_WAITLIST_CAP {
                    if let Some(evicted) = queue.pop_front() {
                        evictions.insert(evicted);
                    }
                }
            }
        }
        for evicted in evictions {
            self.removed_handles.insert(evicted);
            self.remove_waiting_handle(&evicted);
        }
        for entity in entities {
            self.waiting_entity_to_handles
                .entry(*entity)
                .or_default()
                .push_back(new_handle);
        }

        self.handle_ttls.push_back((Instant::now(), new_handle));
        self.handle_to_required_entities
            .insert(new_handle, entities.clone());

        waitlist_store.queue(new_handle, item);

        new_handle
    }

    pub fn collect_ready_items<T>(
        &mut self,
        now: &Instant,
        waitlist_store: &mut WaitlistStore<T>,
    ) -> Option<Vec<T>> {
        // A handle has exactly one terminal retirement point: the owning store
        // relinquishes its item here (ready or expired), after the handle has
        // also been removed from ready_handles/removed_handles and the
        // dependency indexes. Never recycle from spawn/TTL index cleanup: all
        // channel stores share this allocator, so early reuse lets one store
        // consume another store's readiness marker.
        self.check_handle_ttls(now);
        for handle in waitlist_store.remove_expired_items(&mut self.removed_handles) {
            self.handle_store.recycle_key(&handle);
        }

        if self.ready_handles.is_empty() {
            return None;
        }

        let (handles, items) = waitlist_store.collect_ready_items(&mut self.ready_handles)?;
        for handle in handles {
            self.handle_store.recycle_key(&handle);
        }
        Some(items)
    }

    pub fn spawn_entity(
        &mut self,
        in_scope_entities: &dyn InScopeEntities<RemoteEntity>,
        // converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &RemoteEntity,
    ) {
        // let remote_entity = converter.global_entity_to_remote_entity(global_entity).unwrap();
        // warn!("Waitlist is tracking in-scope entity ({:?}, {:?}) .. should have been added to GlobalWorldManager", remote_entity, global_entity);

        // get a list of handles ready to send
        let mut outgoing_handles = Vec::new();

        if let Some(message_set) = self.waiting_entity_to_handles.get(entity) {
            for message_handle in message_set.iter() {
                if let Some(entities) = self.handle_to_required_entities.get(message_handle) {
                    if self.required_entities_are_in_scope(in_scope_entities, entities) {
                        // info!("Entity's dependencies {:?} are in scope", entities);
                        outgoing_handles.push(*message_handle);
                    }
                }
            }
        }

        // get the messages ready to send, also clean up
        for outgoing_handle in outgoing_handles {
            // push outgoing message
            self.ready_handles.insert(outgoing_handle);
            self.remove_waiting_handle(&outgoing_handle);
        }
    }

    pub fn despawn_entity(&mut self, _entity: &RemoteEntity) {
        // stub
    }

    pub fn remove_waiting_handle(&mut self, handle: &WaitlistHandle) {
        // remove handle from ttl list
        if let Some(ttl_index) = self
            .handle_ttls
            .iter()
            .position(|(_, ttl_handle)| ttl_handle == handle)
        {
            self.handle_ttls.remove(ttl_index);
        }

        // remove handle from required entities map
        let entities = self.handle_to_required_entities.remove(handle).unwrap();

        // for all associated entities, remove from waitlist
        for entity in entities {
            let mut remove = false;
            if let Some(queue) = self.waiting_entity_to_handles.get_mut(&entity) {
                queue.retain(|h| h != handle);
                if queue.is_empty() {
                    remove = true;
                }
            }
            if remove {
                self.waiting_entity_to_handles.remove(&entity);
            }
        }
    }

    fn check_handle_ttls(&mut self, now: &Instant) {
        loop {
            let Some((ttl, _)) = self.handle_ttls.front() else {
                break;
            };
            if ttl.elapsed(now) < self.handle_ttl {
                break;
            }
            let (_, handle) = self.handle_ttls.pop_front().unwrap();
            self.removed_handles.insert(handle);
            self.remove_waiting_handle(&handle);
        }
    }
}

pub struct WaitlistStore<T> {
    item_handles: HashSet<WaitlistHandle>,
    items: HashMap<WaitlistHandle, T>,
}

impl<T> WaitlistStore<T> {
    pub fn new() -> Self {
        Self {
            item_handles: HashSet::new(),
            items: HashMap::new(),
        }
    }

    pub fn queue(&mut self, handle: WaitlistHandle, item: T) {
        self.item_handles.insert(handle);
        self.items.insert(handle, item);
    }

    pub fn collect_ready_items(
        &mut self,
        ready_handles: &mut HashSet<WaitlistHandle>,
    ) -> Option<(Vec<WaitlistHandle>, Vec<T>)> {
        let intersection: HashSet<WaitlistHandle> = self
            .item_handles
            .intersection(ready_handles)
            .cloned()
            .collect();

        if intersection.is_empty() {
            // Handles in ready_handles must refer to items in another WaitlistStore
            return None;
        }

        let mut collected_handles = Vec::with_capacity(intersection.len());
        let mut ready_messages = Vec::with_capacity(intersection.len());

        for handle in intersection {
            ready_handles.remove(&handle);
            let item = self.remove(&handle).unwrap();
            collected_handles.push(handle);
            ready_messages.push(item);
        }

        Some((collected_handles, ready_messages))
    }

    pub fn remove_expired_items(
        &mut self,
        expired_handles: &mut HashSet<WaitlistHandle>,
    ) -> Vec<WaitlistHandle> {
        let intersection: HashSet<WaitlistHandle> = self
            .item_handles
            .intersection(expired_handles)
            .cloned()
            .collect();

        let mut removed = Vec::with_capacity(intersection.len());
        for handle in intersection {
            expired_handles.remove(&handle);
            self.remove(&handle);
            removed.push(handle);
        }
        removed
    }

    pub fn remove(&mut self, handle: &WaitlistHandle) -> Option<T> {
        self.item_handles.remove(handle);
        self.items.remove(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Scope(HashSet<RemoteEntity>);

    impl InScopeEntities<RemoteEntity> for Scope {
        fn has_entity(&self, entity: &RemoteEntity) -> bool {
            self.0.contains(entity)
        }
    }

    fn dependencies(entity: RemoteEntity) -> HashSet<RemoteEntity> {
        HashSet::from([entity])
    }

    #[test]
    fn another_store_cannot_consume_and_strand_a_ready_item() {
        let mut waitlist = RemoteEntityWaitlist::with_recycle_timeout(Duration::ZERO);
        let mut scope = Scope::default();
        let mut store_a = WaitlistStore::new();
        let mut store_b = WaitlistStore::new();
        let entity_a = RemoteEntity::new(1);
        let entity_b = RemoteEntity::new(2);

        waitlist.queue(&scope, &dependencies(entity_a), &mut store_a, "a");
        scope.0.insert(entity_a);
        waitlist.spawn_entity(&scope, &entity_a);
        waitlist.queue(&scope, &dependencies(entity_b), &mut store_b, "b");

        // Channel B polls first. It must not consume A's global ready marker;
        // A must still collect its own item exactly once.
        assert_eq!(
            store_b.collect_ready_items(&mut waitlist.ready_handles),
            None,
        );
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut store_a),
            Some(vec!["a"]),
        );
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut store_a),
            None,
        );
    }

    #[test]
    fn not_ready_item_is_not_delivered_by_a_recycled_ready_marker() {
        let mut waitlist = RemoteEntityWaitlist::with_recycle_timeout(Duration::ZERO);
        let mut scope = Scope::default();
        let mut store_a = WaitlistStore::new();
        let mut store_b = WaitlistStore::new();
        let entity_a = RemoteEntity::new(1);
        let entity_b = RemoteEntity::new(2);

        waitlist.queue(&scope, &dependencies(entity_a), &mut store_a, "a");
        scope.0.insert(entity_a);
        waitlist.spawn_entity(&scope, &entity_a);

        // Queue B after A becomes ready but before A's store is polled. B must
        // receive a distinct handle and must not inherit A's ready marker.
        waitlist.queue(&scope, &dependencies(entity_b), &mut store_b, "b");
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut store_b),
            None
        );
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut store_a),
            Some(vec!["a"]),
        );
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut store_a),
            None
        );

        scope.0.insert(entity_b);
        waitlist.spawn_entity(&scope, &entity_b);
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut store_b),
            Some(vec!["b"]),
        );
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut store_b),
            None
        );
    }

    #[test]
    fn ttl_retirement_cannot_consume_or_strand_another_store_ready_handle() {
        let mut waitlist = RemoteEntityWaitlist::with_recycle_timeout(Duration::ZERO);
        let mut scope = Scope::default();
        let mut expired_store = WaitlistStore::new();
        let mut unrelated_store: WaitlistStore<&str> = WaitlistStore::new();
        let mut later_store = WaitlistStore::new();
        let expired_entity = RemoteEntity::new(10);
        let later_entity = RemoteEntity::new(11);

        waitlist.queue(
            &scope,
            &dependencies(expired_entity),
            &mut expired_store,
            "expired",
        );
        waitlist.handle_ttl = Duration::ZERO;

        // Let A expire while polling an unrelated store. The global removed
        // marker remains until A's owning store relinquishes A.
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut unrelated_store),
            None,
        );
        waitlist.handle_ttl = Duration::from_secs(60);

        waitlist.queue(
            &scope,
            &dependencies(later_entity),
            &mut later_store,
            "later",
        );

        // A's stale removed marker must not evict B from a different store.
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut later_store),
            None,
        );
        scope.0.insert(later_entity);
        waitlist.spawn_entity(&scope, &later_entity);
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut later_store),
            Some(vec!["later"]),
        );

        // The expired item remains owned by A until A is polled, then retires
        // without affecting any other store.
        assert_eq!(
            waitlist.collect_ready_items(&Instant::now(), &mut expired_store),
            None,
        );
    }
}
