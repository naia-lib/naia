use std::time::Duration;

use naia_shared::{LinkConditionerConfig, Protocol, Serde};

mod auth;
mod character;
mod string_message;
mod basic_request;

pub use auth::Auth;
pub use character::Character;
pub use string_message::StringMessage;
pub use basic_request::{BasicRequest, BasicResponse};

#[derive(Serde, PartialEq, Clone, Default)]
pub struct MyMarker;

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
        .add_message::<StringMessage<MyMarker>>()
        // Requests
        .add_request::<BasicRequest>()
        // Components
        .add_component::<Character<MyMarker>>()
        // Build Protocol
        .build()
}
