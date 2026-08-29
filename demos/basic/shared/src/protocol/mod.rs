use std::time::Duration;

#[cfg(debug_assertions)]
use naia_shared::LinkConditionerConfig;
use naia_shared::Protocol;

mod auth;
mod basic_request;
mod character;
mod string_message;

pub use auth::Auth;
pub use basic_request::{BasicRequest, BasicResponse};
pub use character::Character;
pub use string_message::StringMessage;

// Protocol Build
pub fn protocol() -> Protocol {
    let mut builder = Protocol::builder();
    builder
        // Config
        .tick_interval(Duration::from_millis(800))
        // Channels
        .add_default_channels()
        // Messages
        .add_message::<Auth>()
        .add_message::<StringMessage>()
        // Requests
        .add_request::<BasicRequest>()
        // Components
        .add_component::<Character>();

    // The link conditioner simulates latency, jitter and packet loss. That is
    // what you want while developing, and almost never what you want in a
    // shipped build -- so gate it on the build profile rather than remembering
    // to delete it. Naia deliberately leaves the choice to the app (see
    // naia-lib/naia#65); this is just the pattern.
    #[cfg(debug_assertions)]
    builder.link_condition(LinkConditionerConfig::average_condition());

    builder.build()
}
