use naia_shared::{ProtocolPlugin, Protocol};

mod auth;
mod entity_assignment;
mod key_command;

pub use auth::Auth;
pub use entity_assignment::EntityAssignment;
pub use key_command::KeyCommand;

// Plugin
pub struct MessagesPlugin;

impl ProtocolPlugin for MessagesPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol
            .add_message::<Auth>()
            .add_message::<EntityAssignment>()
            .add_message::<KeyCommand>();
    }
}
