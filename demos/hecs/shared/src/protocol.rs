use naia_shared::Protocolize;

mod auth;
mod marker;
mod name;
mod position;

pub use auth::Auth;
pub use marker::Marker;
pub use name::Name;
pub use position::Position;

#[derive(Protocolize)]
pub enum Protocol {
    Auth(Auth),
    Name(Name),
    Position(Position),
    Marker(Marker),
}
