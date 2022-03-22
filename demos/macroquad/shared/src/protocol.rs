use naia_shared::Protocolize;

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

#[derive(Protocolize)]
pub enum Protocol {
    Auth(Auth),
    EntityAssignment(EntityAssignment),
    KeyCommand(KeyCommand),
    Square(Square),
    Marker(Marker),
}
