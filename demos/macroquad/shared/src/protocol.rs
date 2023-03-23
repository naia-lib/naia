use std::time::Duration;

use naia_shared::{LinkConditionerConfig, Protocol};

use crate::{channels::ChannelsPlugin, components::ComponentsPlugin, messages::MessagesPlugin};

// Protocol Build
pub fn protocol() -> Protocol {
    Protocol::builder()
        // Config
        .tick_interval(Duration::from_millis(250))
        .link_condition(LinkConditionerConfig::new(100, 0, 0.0))
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
