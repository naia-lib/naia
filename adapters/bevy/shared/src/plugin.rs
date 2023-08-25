use bevy_app::{App, Plugin as PluginType, Update};
use bevy_ecs::schedule::IntoSystemConfigs;
use bevy_ecs::schedule::IntoSystemSetConfig;

use crate::{
    change_detection::{on_despawn, HostSyncEvent},
    system_set::HostSyncChangeTracking,
    BeforeReceiveEvents, ReceiveEvents,
};

pub struct SharedPlugin;

impl PluginType for SharedPlugin {
    fn build(&self, app: &mut App) {
        app
            // EVENTS //
            .add_event::<HostSyncEvent>()
            // SYSTEM SETS //
            .configure_set(Update, HostSyncChangeTracking.before(BeforeReceiveEvents))
            .configure_set(Update, BeforeReceiveEvents.before(ReceiveEvents))
            // SYSTEMS //
            .add_systems(Update, on_despawn.in_set(HostSyncChangeTracking));
    }
}
