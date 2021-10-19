use naia_derive::ProtocolType;
use naia_shared::Manifest;

mod auth;
mod color;
mod key_command;
mod position;

pub use auth::Auth;
pub use color::{Color, ColorValue};
pub use key_command::KeyCommand;
pub use position::Position;

#[derive(ProtocolType, Clone)]
pub enum Protocol {
    Auth(Auth),
    KeyCommand(KeyCommand),
    Position(Position),
    Color(Color),
}

pub enum ProtocolKind {
    Auth,
    KeyCommand,
    Position,
    Color,
}

pub enum ProtocolRef<'a> {
    Auth(&'a Auth),
    KeyCommand(&'a KeyCommand),
    Position(&'a Position),
    Color(&'a Color),
}

pub enum ProtocolMut<'a> {
    Auth(&'a mut Auth),
    KeyCommand(&'a mut KeyCommand),
    Position(&'a mut Position),
    Color(&'a mut Color),
}
