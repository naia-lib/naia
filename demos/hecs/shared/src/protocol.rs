use std::time::Duration;

use naia_hecs_shared::{LinkConditionerConfig, Protocol};

mod auth;
mod marker;
mod name;
mod position;

pub use auth::Auth;
pub use marker::Marker;
pub use name::Name;
pub use position::Position;

// Protocol Build
pub fn protocol() -> Protocol {
    Protocol::builder()
        // Config
        .tick_interval(Duration::from_millis(25))
        .link_condition(LinkConditionerConfig::average_condition())
        // Channels
        .add_default_channels()
        // Messages
        .add_message::<Auth>()
        // Components
        .add_component::<Marker>()
        .add_component::<Name>()
        .add_component::<Position>()
        // Build Protocol
        .build()
}
