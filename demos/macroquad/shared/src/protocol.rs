use naia_derive::Protocolize;

mod auth;
mod key_command;
mod square;

pub use auth::Auth;
pub use key_command::KeyCommand;
pub use square::{Color, Square};

#[derive(Protocolize)]
pub enum Protocol {
    Auth(Auth),
    KeyCommand(KeyCommand),
    Square(Square),
}
