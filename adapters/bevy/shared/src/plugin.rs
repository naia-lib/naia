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

impl<T: Send + Sync + 'static> SharedPlugin<T> {
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
        }
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
            // MESSAGES //
            .add_message::<HostSyncEvent>()
            // SYSTEMS //
            .add_systems(
                Update,
                bevy_ecs::system::IntoSystem::into_system(on_host_owned_added).in_set(HostSyncOwnedAddedTracking),
            )
            .add_systems(
                Update,
                bevy_ecs::system::IntoSystem::into_system(on_despawn).in_set(HostSyncChangeTracking),
            );
    }
}
