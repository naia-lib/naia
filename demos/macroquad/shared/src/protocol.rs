use std::time::Duration;

use naia_shared::{
    Channel, ChannelDirection, ChannelMode, LinkConditionerConfig, Protocol, ReliableSettings,
    SocketConfig, TickBufferSettings,
};

mod auth;
mod entity_assignment;
mod key_command;
mod marker;
mod square;

pub use auth::Auth;
pub use entity_assignment::EntityAssignment;
pub use key_command::KeyCommand;
pub use marker::Marker;
pub use square::{Color, Square};

pub struct PlayerCommandChannel;
//TODO: implement derive!
impl Channel for PlayerCommandChannel {}
pub struct EntityAssignmentChannel;
//TODO: implement derive!
impl Channel for EntityAssignmentChannel {}

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
