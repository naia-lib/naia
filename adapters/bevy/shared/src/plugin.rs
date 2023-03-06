use bevy_app::{App, Plugin as PluginType};
use bevy_ecs::schedule::{IntoSystemConfig, IntoSystemSetConfig};

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
            .configure_set(HostSyncChangeTracking.before(BeforeReceiveEvents))
            .configure_set(BeforeReceiveEvents.before(ReceiveEvents))
            // SYSTEMS //
            .add_system(on_despawn.in_set(HostSyncChangeTracking));
    }
}
