
use naia_derive::StateType;
use naia_shared::Ref;

mod position;
pub use position::Position;

mod name;
pub use name::Name;

mod marker;
pub use marker::Marker;

#[derive(StateType, Clone)]
pub enum Components {
    Position(Ref<Position>),
    Name(Ref<Name>),
    Marker(Ref<Marker>),
}
