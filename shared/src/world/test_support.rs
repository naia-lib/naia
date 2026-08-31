//! Shared doubles for the `world` unit suites.
//!
//! `GlobalWorldManagerType` is a wide trait that almost every world type takes
//! a `&dyn` reference to, so each suite that wants to drive one of them needs a
//! stand-in. Keeping a single copy here means the doubles cannot drift apart
//! between suites -- a divergence would make two suites disagree about what
//! "the world manager" does while both stayed green.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use crate::{
    world::{
        delegation::auth_channel::EntityAuthAccessor,
        update::{
            global_diff_handler::GlobalDiffHandler,
            global_dirty_bitset::GlobalDirtyBitset,
            mut_channel::{MutChannelType, MutReceiver},
        },
    },
    ComponentKind, ComponentKinds, GlobalEntity, GlobalWorldManagerType, InScopeEntities,
    PropertyMutator,
};

/// A mut channel that hands every address the same receiver, so a test can
/// arm a diff mask and then observe it through any of them.
pub struct TestMutChannel {
    diff_mask_length: u8,
    receivers: Vec<MutReceiver>,
    receiver_index: HashMap<SocketAddr, usize>,
}

impl MutChannelType for TestMutChannel {
    fn new_receiver(&mut self, address_opt: &Option<SocketAddr>) -> Option<MutReceiver> {
        let address = address_opt.expect("test channel requires an address");
        if let Some(&idx) = self.receiver_index.get(&address) {
            return Some(self.receivers[idx].clone());
        }
        let receiver = MutReceiver::new(self.diff_mask_length);
        let idx = self.receivers.len();
        self.receivers.push(receiver.clone());
        self.receiver_index.insert(address, idx);
        Some(receiver)
    }

    fn send(&self, property_index: u8) {
        for receiver in &self.receivers {
            receiver.mutate(property_index);
        }
    }
}

/// A permissive `GlobalWorldManagerType`: every entity is in scope, replicating,
/// non-static, and relatable to every user. Methods no suite drives are
/// `unreachable!` rather than defaulted, so a caller that starts reaching for
/// them fails loudly instead of silently getting a stub answer.
pub struct TestGwm {
    /// The diff handler this manager hands out. Exposed so a test can arm a
    /// component's diff mask directly.
    pub diff_handler: Arc<RwLock<GlobalDiffHandler>>,
    global_dirty: Arc<GlobalDirtyBitset>,
    /// What `component_kinds` reports per entity. Empty by default, so a test
    /// that does not opt in sees the "entity has no components yet" branch.
    declared_kinds: RwLock<HashMap<GlobalEntity, Vec<ComponentKind>>>,
}

impl TestGwm {
    /// Builds a manager whose diff handler is sized for `kinds`.
    pub fn new(kinds: &ComponentKinds) -> Self {
        let diff_handler = Arc::new(RwLock::new(GlobalDiffHandler::new()));
        diff_handler
            .write()
            .unwrap()
            .set_protocol_kind_count(kinds.kind_count());
        Self {
            diff_handler,
            global_dirty: Arc::new(GlobalDirtyBitset::new(64, kinds.kind_count() as usize)),
            declared_kinds: RwLock::new(HashMap::new()),
        }
    }

    /// Gives the global diff handler a live receiver for `(entity, kind)`, so a
    /// later `register_component` on the ledger has something to find.
    pub fn arm_diff_handler(
        &self,
        kinds: &ComponentKinds,
        entity: &GlobalEntity,
        kind: &ComponentKind,
    ) {
        let mut gdh = self.diff_handler.write().unwrap();
        if gdh.kind_bit(kind).is_none() {
            gdh.alloc_entity(*entity);
        }
        gdh.register_component(kinds, self, entity, kind, 1);
    }

    /// Declares the component kinds `component_kinds` should report for
    /// `entity`, which is what the authority-grant path iterates over.
    pub fn declare_kinds(&self, entity: &GlobalEntity, kinds: Vec<ComponentKind>) {
        self.declared_kinds.write().unwrap().insert(*entity, kinds);
    }
}

impl InScopeEntities<GlobalEntity> for TestGwm {
    fn has_entity(&self, _: &GlobalEntity) -> bool {
        true
    }
}

impl GlobalWorldManagerType for TestGwm {
    fn component_kinds(&self, entity: &GlobalEntity) -> Option<Vec<ComponentKind>> {
        self.declared_kinds.read().unwrap().get(entity).cloned()
    }
    fn entity_can_relate_to_user(&self, _: &GlobalEntity, _: &u64) -> bool {
        true
    }
    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>> {
        Arc::new(RwLock::new(TestMutChannel {
            diff_mask_length,
            receivers: Vec::new(),
            receiver_index: HashMap::new(),
        }))
    }
    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>> {
        self.diff_handler.clone()
    }
    fn register_component(
        &self,
        _: &ComponentKinds,
        _: &GlobalEntity,
        _: &ComponentKind,
        _: u8,
    ) -> PropertyMutator {
        unreachable!("not exercised by these tests")
    }
    fn get_entity_auth_accessor(&self, _: &GlobalEntity) -> EntityAuthAccessor {
        unreachable!("not exercised by these tests")
    }
    fn entity_needs_mutator_for_delegation(&self, _: &GlobalEntity) -> bool {
        false
    }
    fn entity_is_replicating(&self, _: &GlobalEntity) -> bool {
        true
    }
    fn entity_is_static(&self, _: &GlobalEntity) -> bool {
        false
    }
    fn global_dirty_bitset(&self) -> Option<Arc<GlobalDirtyBitset>> {
        Some(self.global_dirty.clone())
    }
}
