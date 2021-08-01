
use naia_derive::ActorType;
use naia_shared::Ref;

mod point;
pub use point::{Color, Point};

#[derive(ActorType, Clone)]
pub enum State {
    Point(Ref<Point>),
}
