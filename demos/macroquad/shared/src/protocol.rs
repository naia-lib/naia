use std::time::Duration;

use naia_shared::{
    Channel, ChannelDirection, ChannelMode, LinkConditionerConfig, Protocol, ReliableSettings,
    SocketConfig, TickBufferSettings,
};

use crate::{
    channels::{EntityAssignmentChannel, PlayerCommandChannel},
    components::{Marker, Square},
    messages::{Auth, EntityAssignment, KeyCommand},
};

// Protocol Build
pub fn protocol() -> Protocol {
    Protocol::builder()
        // Config
        .tick_interval(Duration::from_millis(20))
        .link_condition(LinkConditionerConfig::average_condition())
        // Channels
        .add_channel::<PlayerCommandChannel>(
            ChannelDirection::ClientToServer,
            ChannelMode::TickBuffered(TickBufferSettings::default()),
        )
        .add_channel::<EntityAssignmentChannel>(
            ChannelDirection::ServerToClient,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        // Messages
        .add_message::<Auth>()
        .add_message::<EntityAssignment>()
        .add_message::<KeyCommand>()
        // Components
        .add_component::<Square>()
        .add_component::<Marker>()
        // Build
        .build()
}
