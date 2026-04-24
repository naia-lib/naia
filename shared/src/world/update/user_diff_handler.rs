use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use log::warn;

use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::global_diff_handler::GlobalDiffHandler;
use crate::world::update::mut_channel::MutReceiver;

// Diagnostic counters for the perf-upgrade project. These measure how much
// work `dirty_receiver_candidates` does per invocation. On idle ticks today
// the scan is O(receivers), which multiplied by users is the O(U·N) cost the
// matrix shows. After Phase 3 lands a dirty-push model, `receivers_visited`
// per idle tick should drop to zero. Enabled via `bench_instrumentation`.
#[cfg(feature = "bench_instrumentation")]
pub mod dirty_scan_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    pub static SCAN_CALLS: AtomicU64 = AtomicU64::new(0);
    pub static RECEIVERS_VISITED: AtomicU64 = AtomicU64::new(0);
    pub static DIRTY_RESULTS: AtomicU64 = AtomicU64::new(0);

    pub fn reset() {
        SCAN_CALLS.store(0, Ordering::Relaxed);
        RECEIVERS_VISITED.store(0, Ordering::Relaxed);
        DIRTY_RESULTS.store(0, Ordering::Relaxed);
    }
    pub fn snapshot() -> (u64, u64, u64) {
        (
            SCAN_CALLS.load(Ordering::Relaxed),
            RECEIVERS_VISITED.load(Ordering::Relaxed),
            DIRTY_RESULTS.load(Ordering::Relaxed),
        )
    }
}

#[derive(Clone)]
pub struct UserDiffHandler {
    receivers: HashMap<(GlobalEntity, ComponentKind), MutReceiver>,
    global_diff_handler: Arc<RwLock<GlobalDiffHandler>>,
}

impl UserDiffHandler {
    pub fn new(global_world_manager: &dyn GlobalWorldManagerType) -> Self {
        Self {
            receivers: HashMap::new(),
            global_diff_handler: global_world_manager.diff_handler(),
        }
    }

    // Component Registration
    pub fn register_component(
        &mut self,
        address: &Option<SocketAddr>,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        let Ok(global_handler) = self.global_diff_handler.as_ref().read() else {
            panic!("Be sure you can get self.global_diff_handler before calling this!");
        };
        let Some(receiver) = global_handler.receiver(address, entity, component_kind) else {
            // Component not yet registered in GlobalDiffHandler - this can happen on the client
            // side when authority is granted before components are registered for diff tracking.
            // Skip registration for now; it will be registered when the component is actually
            // inserted or when it needs to be diffed.
            #[cfg(feature = "e2e_debug")]
            {
                warn!(
                    "UserDiffHandler: Component {:?} for {:?} not yet registered in GlobalDiffHandler, skipping registration",
                    component_kind, entity
                );
            }
            return;
        };

        self.receivers.insert((*entity, *component_kind), receiver);
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.receivers.remove(&(*entity, *component_kind));
    }

    pub fn has_component(&self, entity: &GlobalEntity, component: &ComponentKind) -> bool {
        self.receivers.contains_key(&(*entity, *component))
    }

    // Diff masks
    pub fn diff_mask(
        &'_ self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> RwLockReadGuard<'_, DiffMask> {
        let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        return receiver.mask();
    }

    pub fn diff_mask_is_clear(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) else {
            warn!(
                "diff_mask_is_clear(): Should not call this unless we're sure there's a receiver"
            );
            return true;
        };
        return receiver.diff_mask_is_clear();
    }

    pub fn or_diff_mask(
        &mut self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
        other_mask: &DiffMask,
    ) {
        let Some(receiver) = self.receivers.get_mut(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.or_mask(other_mask);
    }

    pub fn clear_diff_mask(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some(receiver) = self.receivers.get_mut(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.clear_mask();
    }

    #[cfg(feature = "test_utils")]
    pub fn receiver_count(&self) -> usize {
        self.receivers.len()
    }

    #[cfg(feature = "test_utils")]
    pub fn dirty_candidates_count(&self) -> usize {
        self.receivers.values().filter(|r| !r.diff_mask_is_clear()).count()
    }

    pub fn dirty_receiver_candidates(&self) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
        #[cfg(feature = "bench_instrumentation")]
        {
            use std::sync::atomic::Ordering;
            dirty_scan_counters::SCAN_CALLS.fetch_add(1, Ordering::Relaxed);
            dirty_scan_counters::RECEIVERS_VISITED
                .fetch_add(self.receivers.len() as u64, Ordering::Relaxed);
        }
        let mut result: HashMap<GlobalEntity, HashSet<ComponentKind>> = HashMap::new();
        for ((entity, kind), receiver) in &self.receivers {
            if !receiver.diff_mask_is_clear() {
                result.entry(*entity).or_default().insert(*kind);
            }
        }
        #[cfg(feature = "bench_instrumentation")]
        {
            use std::sync::atomic::Ordering;
            let dirty: u64 = result.values().map(|s| s.len() as u64).sum();
            dirty_scan_counters::DIRTY_RESULTS.fetch_add(dirty, Ordering::Relaxed);
        }
        result
    }
}
