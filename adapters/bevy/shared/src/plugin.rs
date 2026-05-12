use std::marker::PhantomData;

use bevy_app::{App, Plugin as PluginType, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;

use log::info;

use crate::{
    change_detection::{on_despawn, on_host_owned_added, HostSyncEvent},
    system_set::{HostSyncChangeTracking, HostSyncOwnedAddedTracking},
    HostOwnedMap,
};

pub struct SharedPlugin<T: Send + Sync + 'static> {
    phantom_t: PhantomData<T>,
}

impl<T: Send + Sync + 'static> Default for SharedPlugin<T> {
    fn default() -> Self {
        Self {
            phantom_t: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static> SharedPlugin<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Send + Sync + 'static> PluginType for SharedPlugin<T> {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<Self>() {
            info!("attempted to add SharedPlugin twice to App");
            return;
        }
        app
            // RESOURCES //
            .init_resource::<HostOwnedMap>()
            // EVENTS //
            .add_message::<HostSyncEvent>()
            // SYSTEMS //
            .add_systems(
                Update,
                on_host_owned_added.in_set(HostSyncOwnedAddedTracking),
            )
            .add_systems(Update, on_despawn.in_set(HostSyncChangeTracking))
            // Force-order: `on_host_owned_added` populates HostOwnedMap
            // for newly-replicated entities; `on_despawn` panics if it
            // runs first on a same-frame spawn-and-despawn (the entity's
            // HostOwned was added then removed before the map was
            // written). Constraint matches the data dependency: the map
            // must be written before any read.
            .configure_sets(
                Update,
                HostSyncOwnedAddedTracking.before(HostSyncChangeTracking),
            );
    }
}
