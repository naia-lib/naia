use std::time::Duration;

use naia_bevy_shared::{LinkConditionerConfig, Protocol};

use crate::{channels::ChannelsPlugin, components::ComponentsPlugin, messages::MessagesPlugin};

// Protocol Build
pub fn protocol() -> Protocol {
    Protocol::builder()
        // Config
        .tick_interval(Duration::from_millis(40))
        .link_condition(LinkConditionerConfig::poor_condition())
        .enable_client_authoritative_entities()
        // Channels
        .add_plugin(ChannelsPlugin)
        // Messages
        .add_plugin(MessagesPlugin)
        // Components
        .add_plugin(ComponentsPlugin)
        // Build Protocol
        .build()
}
