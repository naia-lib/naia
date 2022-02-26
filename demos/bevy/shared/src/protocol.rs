use naia_shared::Protocolize;

mod auth;
mod color;
mod key_command;
mod position;

pub use auth::Auth;
pub use color::{Color, ColorValue};
pub use key_command::KeyCommand;
pub use position::Position;

#[derive(Protocolize)]
pub enum Protocol {
    Auth(Auth),
    KeyCommand(KeyCommand),
    Position(Position),
    Color(Color),
}
