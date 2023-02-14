use std::time::Duration;

use naia_shared::{LinkConditionerConfig, Protocol};

mod auth;
mod character;
mod string_message;

pub use auth::Auth;
pub use character::Character;
pub use string_message::StringMessage;

// Protocol Build
pub fn protocol() -> Protocol {
    Protocol::builder()
        // Config
        .tick_interval(Duration::from_millis(800))
        .link_condition(LinkConditionerConfig::average_condition())
        // Channels
        .add_default_channels()
        // Messages
        .add_message::<Auth>()
        .add_message::<StringMessage>()
        // Components
        .add_component::<Character>()
        // Build Protocol
        .build()
}
