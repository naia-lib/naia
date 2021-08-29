use naia_derive::ProtocolType;
use naia_shared::{Manifest, Ref};

mod auth;
mod key_command;
mod color;
mod position;

pub use auth::Auth;
pub use key_command::KeyCommand;
pub use position::Position;
pub use color::{Color, ColorValue};

#[derive(ProtocolType, Clone)]
pub enum Protocol {
    Auth(Ref<Auth>),
    KeyCommand(Ref<KeyCommand>),
    Position(Ref<Position>),
    Color(Ref<Color>),
}
