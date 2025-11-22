use naia_shared::{
    ComponentKind, EntityAuthAccessor, GlobalDiffHandler, GlobalEntity, GlobalWorldManagerType,
    InScopeEntities, MutChannelType, PropertyMutator,
};
use std::sync::{Arc, RwLock};

/// Minimal test implementation of GlobalWorldManagerType
/// This is a stub for testing integration flows that don't require full world management
pub struct TestGlobalWorldManager {
    diff_handler: Arc<RwLock<GlobalDiffHandler>>,
}

impl TestGlobalWorldManager {
    pub fn new() -> Self {
        Self {
            diff_handler: Arc::new(RwLock::new(GlobalDiffHandler::new())),
        }
    }
}

impl InScopeEntities<GlobalEntity> for TestGlobalWorldManager {
    fn has_entity(&self, _entity: &GlobalEntity) -> bool {
        true // Always return true for testing
    }
}

impl GlobalWorldManagerType for TestGlobalWorldManager {
    fn component_kinds(&self, _entity: &GlobalEntity) -> Option<Vec<ComponentKind>> {
        Some(Vec::new())
    }

    fn entity_can_relate_to_user(&self, _global_entity: &GlobalEntity, _user_key: &u64) -> bool {
        true
    }

    fn new_mut_channel(&self, _diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>> {
        unimplemented!("new_mut_channel not needed for these tests")
    }

    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>> {
        self.diff_handler.clone()
    }

    fn register_component(
        &self,
        _component_kinds: &naia_shared::ComponentKinds,
        _global_entity: &GlobalEntity,
        _component_kind: &ComponentKind,
        _diff_mask_length: u8,
    ) -> PropertyMutator {
        unimplemented!("register_component not needed for these tests")
    }

    fn get_entity_auth_accessor(&self, _global_entity: &GlobalEntity) -> EntityAuthAccessor {
        unimplemented!(
            "get_entity_auth_accessor not needed for basic tests - use channel status instead"
        )
    }

    fn entity_needs_mutator_for_delegation(&self, _global_entity: &GlobalEntity) -> bool {
        false
    }

    fn entity_is_replicating(&self, _global_entity: &GlobalEntity) -> bool {
        true
    }
}
