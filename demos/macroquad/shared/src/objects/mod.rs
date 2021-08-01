
use naia_derive::StateType;
use naia_shared::Ref;

mod point;
pub use point::{Color, Point};

#[derive(StateType, Clone)]
pub enum Objects {
    Point(Ref<Point>),
}
