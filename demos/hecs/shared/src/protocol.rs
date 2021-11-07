use naia_derive::ProtocolType;

mod auth;
mod marker;
mod name;
mod position;
mod string_message;

pub use auth::Auth;
pub use marker::Marker;
pub use name::Name;
pub use position::Position;
pub use string_message::StringMessage;

#[derive(ProtocolType)]
pub enum Protocol {
    Position(Position),
    Name(Name),
    Marker(Marker),
    StringMessage(StringMessage),
    Auth(Auth),
}
