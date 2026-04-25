use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use log::warn;

use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::global_diff_handler::GlobalDiffHandler;
use crate::world::update::mut_channel::{DirtyNotifier, DirtySet, MutReceiver};

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
    // Dirty-set: the (entity, kind) keys whose MutReceivers currently hold a
    // non-clear DiffMask. Maintained incrementally by DirtyNotifier callbacks
    // fired inside MutReceiver::mutate / or_mask / clear_mask. The scan in
    // `dirty_receiver_candidates` reads this directly — O(dirty), not O(N).
    dirty_set: Arc<DirtySet>,
}

impl UserDiffHandler {
    pub fn new(global_world_manager: &dyn GlobalWorldManagerType) -> Self {
        Self {
            receivers: HashMap::new(),
            global_diff_handler: global_world_manager.diff_handler(),
            dirty_set: Arc::new(RwLock::new(HashMap::new())),
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

        receiver.attach_notifier(DirtyNotifier::new(
            *entity,
            *component_kind,
            Arc::downgrade(&self.dirty_set),
        ));
        self.receivers.insert((*entity, *component_kind), receiver);
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.receivers.remove(&(*entity, *component_kind));
        if let Ok(mut set) = self.dirty_set.write() {
            if let Some(kinds) = set.get_mut(entity) {
                kinds.remove(component_kind);
                if kinds.is_empty() {
                    set.remove(entity);
                }
            }
        }
    }

    pub fn has_component(&self, entity: &GlobalEntity, component: &ComponentKind) -> bool {
        self.receivers.contains_key(&(*entity, *component))
    }

    // Diff masks
    pub fn diff_mask_snapshot(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> DiffMask {
        let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.mask_snapshot()
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
        let result: HashMap<GlobalEntity, HashSet<ComponentKind>> = match self.dirty_set.read() {
            Ok(guard) => guard.clone(),
            Err(_) => HashMap::new(),
        };
        #[cfg(feature = "bench_instrumentation")]
        {
            use std::sync::atomic::Ordering;
            dirty_scan_counters::SCAN_CALLS.fetch_add(1, Ordering::Relaxed);
            // With the dirty-push model, this is the number of dirty entries we
            // actually touched — O(dirty), not O(receivers). If the ratio to
            // (U·N) is > 0.01 on an idle tick, Phase 3 has regressed.
            let visited: u64 = result.values().map(|s| s.len() as u64).sum();
            dirty_scan_counters::RECEIVERS_VISITED.fetch_add(visited, Ordering::Relaxed);
            dirty_scan_counters::DIRTY_RESULTS.fetch_add(visited, Ordering::Relaxed);
        }
        result
    }
}
