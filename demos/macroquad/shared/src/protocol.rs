use naia_derive::ProtocolType;
use naia_shared::{Manifest, Ref};

mod auth;
mod key_command;
mod square;

pub use auth::Auth;
pub use key_command::KeyCommand;
pub use square::{Color, Square};

#[derive(ProtocolType, Clone)]
pub enum Protocol {
    Auth(Ref<Auth>),
    KeyCommand(Ref<KeyCommand>),
    Square(Ref<Square>),
}
