
use naia_derive::ProtocolType;
use naia_shared::Ref;

mod point;
pub use point::{Color, Point};

#[derive(ProtocolType, Clone)]
pub enum Objects {
    Point(Ref<Point>),
}
