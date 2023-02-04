use naia_shared::{Plugin, ProtocolBuilder};

mod auth;
mod entity_assignment;
mod key_command;

pub use auth::Auth;
pub use entity_assignment::EntityAssignment;
pub use key_command::KeyCommand;

// Plugin
pub struct MessagesPlugin;

impl Plugin for MessagesPlugin {
    fn build(&self, protocol: &mut ProtocolBuilder) {
        protocol
            .add_message::<Auth>()
            .add_message::<EntityAssignment>()
            .add_message::<KeyCommand>();
    }
}
