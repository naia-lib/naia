use std::time::Duration;

#[cfg(debug_assertions)]
use naia_bevy_shared::LinkConditionerConfig;
use naia_bevy_shared::Protocol;

use crate::{channels::ChannelsPlugin, components::ComponentsPlugin, messages::MessagesPlugin};

// Protocol Build
pub fn protocol() -> Protocol {
    let mut builder = Protocol::builder();
    builder
        // Config
        .tick_interval(Duration::from_millis(40))
        .enable_client_authoritative_entities()
        // Channels
        .add_plugin(ChannelsPlugin)
        // Messages
        .add_plugin(MessagesPlugin)
        // Components
        .add_plugin(ComponentsPlugin);

    // The link conditioner simulates latency, jitter and packet loss. That is
    // what you want while developing, and almost never what you want in a
    // shipped build -- so gate it on the build profile rather than remembering
    // to delete it. Naia deliberately leaves the choice to the app (see
    // naia-lib/naia#65); this is just the pattern.
    #[cfg(debug_assertions)]
    builder.link_condition(LinkConditionerConfig::poor_condition());

    builder.build()
}
