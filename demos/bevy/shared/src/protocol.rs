use naia_shared::Protocolize;

mod auth;
mod color;
mod entity_assignment;
mod key_command;
mod position;

pub use auth::Auth;
pub use color::{Color, ColorValue};
pub use entity_assignment::EntityAssignment;
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
