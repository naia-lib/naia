use naia_derive::ProtocolType;
use naia_shared::Ref;

mod position;
pub use position::Position;

mod name;
pub use name::Name;

mod marker;
pub use marker::Marker;

#[derive(ProtocolType, Clone)]
pub enum Components {
    Position(Ref<Position>),
    Name(Ref<Name>),
    Marker(Ref<Marker>),
}
