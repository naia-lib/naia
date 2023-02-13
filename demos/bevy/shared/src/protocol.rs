use std::time::Duration;

use naia_bevy_shared::Protocol;
use naia_shared::LinkConditionerConfig;

use crate::{channels::ChannelsPlugin, components::ComponentsPlugin, messages::MessagesPlugin};

// Protocol Build
pub fn protocol() -> Protocol {
    let mut protocol = Protocol::new();
    protocol
        // Config
        .tick_interval(Duration::from_millis(25))
        .link_condition(LinkConditionerConfig::average_condition())
        // Channels
        .add_plugin(ChannelsPlugin)
        // Messages
        .add_plugin(MessagesPlugin)
        // Components
        .add_plugin(ComponentsPlugin);
    protocol
}
