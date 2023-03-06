use bevy_app::{App, Plugin as PluginType};
use bevy_ecs::schedule::IntoSystemSetConfig;

use crate::{
    change_detection::HostComponentEvent, system_set::HostOwnedChangeTracking, BeforeReceiveEvents,
    ReceiveEvents,
};

pub struct SharedPlugin;

impl PluginType for SharedPlugin {
    fn build(&self, app: &mut App) {
        app
            // EVENTS //
            .add_event::<HostComponentEvent>()
            // SYSTEM SETS //
            .configure_set(HostOwnedChangeTracking.before(BeforeReceiveEvents))
            .configure_set(BeforeReceiveEvents.before(ReceiveEvents));
    }
}
