#![no_main]
use libfuzzer_sys::fuzz_target;

use std::sync::{Arc, RwLock};

use naia_serde::BitReader;
use naia_shared::{
    ComponentKind, ComponentKinds, EntityAuthAccessor, GlobalDiffHandler, GlobalEntity,
    GlobalWorldManagerType, HostType, InScopeEntities, LocalWorldManager, MutChannelType,
    PropertyMutator, Tick, WorldReader,
};

// Minimal stub satisfying GlobalWorldManagerType so we can construct a
// LocalWorldManager without a real server/client world.  Only diff_handler()
// is invoked during LocalWorldManager::new(); all other methods are dead paths
// in the read-only decode path exercised by read_world_events.
struct FakeGlobalWorldManager {
    diff_handler: Arc<RwLock<GlobalDiffHandler>>,
}

impl FakeGlobalWorldManager {
    fn new() -> Self {
        Self {
            diff_handler: Arc::new(RwLock::new(GlobalDiffHandler::new())),
        }
    }
}

impl InScopeEntities<GlobalEntity> for FakeGlobalWorldManager {
    fn has_entity(&self, _: &GlobalEntity) -> bool {
        false
    }
}

impl GlobalWorldManagerType for FakeGlobalWorldManager {
    fn component_kinds(&self, _: &GlobalEntity) -> Option<Vec<ComponentKind>> {
        None
    }
    fn entity_can_relate_to_user(&self, _: &GlobalEntity, _: &u64) -> bool {
        false
    }
    fn new_mut_channel(&self, _: u8) -> Arc<RwLock<dyn MutChannelType>> {
        unreachable!("not called during decode")
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
        unreachable!("not called during decode")
    }
    fn get_entity_auth_accessor(&self, _: &GlobalEntity) -> EntityAuthAccessor {
        unreachable!("not called during decode")
    }
    fn entity_needs_mutator_for_delegation(&self, _: &GlobalEntity) -> bool {
        false
    }
    fn entity_is_replicating(&self, _: &GlobalEntity) -> bool {
        false
    }
    fn entity_is_static(&self, _: &GlobalEntity) -> bool {
        false
    }
}

fuzz_target!(|data: &[u8]| {
    let fake_mgr = FakeGlobalWorldManager::new();
    let mut world_manager =
        LocalWorldManager::new(&None, HostType::Client, 0, &fake_mgr);
    let component_kinds = ComponentKinds::new();
    let tick = Tick::default();
    let mut reader = BitReader::new(data);
    let _ = WorldReader::read_world_events(&mut world_manager, &component_kinds, &tick, &mut reader);
});
