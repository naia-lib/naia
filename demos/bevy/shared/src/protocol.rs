use naia_derive::ProtocolType;
use naia_shared::{Manifest, Ref};

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
    Auth(Ref<Auth>),
    KeyCommand(Ref<KeyCommand>),
    Position(Ref<Position>),
    Color(Ref<Color>),
}
