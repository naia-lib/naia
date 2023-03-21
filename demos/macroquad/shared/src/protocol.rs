use std::time::Duration;

use naia_shared::{LinkConditionerConfig, Protocol};

use crate::{channels::ChannelsPlugin, components::ComponentsPlugin, messages::MessagesPlugin};

// Protocol Build
pub fn protocol() -> Protocol {
    Protocol::builder()
        // Config
        .tick_interval(Duration::from_millis(16))
        .link_condition(LinkConditionerConfig::good_condition())
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
