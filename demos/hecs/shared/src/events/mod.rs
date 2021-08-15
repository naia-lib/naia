use naia_derive::EventType;

mod auth;
pub use auth::Auth;

mod string_message;
pub use string_message::StringMessage;

#[derive(EventType, Clone)]
pub enum Events {
    StringMessage(StringMessage),
    Auth(Auth),
}
