use naia_bevy_shared::{Protocol, ProtocolPlugin};

mod auth;
mod basic_request;
mod entity_assignment;
mod key_command;

pub use auth::Auth;
pub use basic_request::{BasicRequest, BasicResponse};
pub use entity_assignment::EntityAssignment;
pub use key_command::KeyCommand;

// Plugin
pub struct MessagesPlugin;

impl ProtocolPlugin for MessagesPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol
            .add_message::<Auth>()
            .add_message::<EntityAssignment>()
            .add_message::<KeyCommand>()
            .add_request::<BasicRequest>();
    }
}
