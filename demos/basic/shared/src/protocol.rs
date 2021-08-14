
use naia_derive::ProtocolType;
use naia_shared::{Ref, Manifest};

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

impl Protocol {
    pub fn load() -> Manifest<Protocol> {
        let mut manifest = Manifest::<Protocol>::new();

        manifest.register_state(Auth::get_builder());
        manifest.register_state(StringMessage::get_builder());
        manifest.register_state(Position::get_builder());
        manifest.register_state(Name::get_builder());
        manifest.register_state(Marker::get_builder());

        manifest
    }
}