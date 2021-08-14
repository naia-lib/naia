
use naia_derive::ProtocolType;
use naia_shared::Ref;

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

#[derive(ProtocolType, Clone)]
pub enum Protocol {
    Position(Ref<Position>),
    Name(Ref<Name>),
    Marker(Ref<Marker>),
    StringMessage(Ref<StringMessage>),
    Auth(Ref<Auth>),
}