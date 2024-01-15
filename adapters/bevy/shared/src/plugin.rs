use std::marker::PhantomData;

use bevy_app::{App, Plugin as PluginType, Update};
use bevy_ecs::schedule::{IntoSystemConfigs, IntoSystemSetConfigs};

use log::warn;

use crate::{
    change_detection::{on_despawn, HostSyncEvent},
    system_set::HostSyncChangeTracking,
    BeforeReceiveEvents, ReceiveEvents,
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
        warn!("FIX THIS NOW CONNOR! shouldn't run this plugin twice! inspect app.world to see if this needs to happen!");
        app
            // EVENTS //
            .add_event::<HostSyncEvent>()
            // SYSTEM SETS //
            .configure_sets(Update, HostSyncChangeTracking.before(BeforeReceiveEvents))
            .configure_sets(Update, BeforeReceiveEvents.before(ReceiveEvents))
            // SYSTEMS //
            .add_systems(Update, on_despawn::<T>.in_set(HostSyncChangeTracking));
    }
}
