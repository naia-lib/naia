use naia_shared::Protocolize;

mod auth;
mod color;
mod key_command;
mod position;
mod entity_assignment;

pub use auth::Auth;
pub use entity_assignment::EntityAssignment;
pub use color::{Color, ColorValue};
pub use key_command::KeyCommand;
pub use position::Position;

#[derive(Protocolize)]
pub enum Protocol {
    Auth(Auth),
    EntityAssignment(EntityAssignment),
    KeyCommand(KeyCommand),
    Position(Position),
    Color(Color),
}
