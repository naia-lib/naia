
use naia_derive::ProtocolType;
use naia_shared::{Ref, Manifest};

mod auth;
mod key_command;
mod point;

pub use auth::Auth;
pub use key_command::KeyCommand;
pub use point::{Point, Color};

#[derive(ProtocolType, Clone)]
pub enum Protocol {
    Auth(Ref<Auth>),
    KeyCommand(Ref<KeyCommand>),
    Point(Ref<Point>),
}