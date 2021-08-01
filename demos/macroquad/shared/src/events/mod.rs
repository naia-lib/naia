
use naia_derive::EventType;

mod auth;
pub use auth::Auth;

mod key_command;
pub use key_command::KeyCommand;

#[derive(EventType, Clone)]
pub enum Events {
    Auth(Auth),
    KeyCommand(KeyCommand),
}
