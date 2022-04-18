use naia_shared::Protocolize;

mod auth;
mod character;
mod string_message;

pub use auth::Auth;
pub use character::Character;
pub use string_message::StringMessage;

#[derive(Protocolize)]
pub enum Protocol {
    Character(Character),
    StringMessage(StringMessage),
    Auth(Auth),
}
