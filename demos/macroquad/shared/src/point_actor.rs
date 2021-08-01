use nanoserde::{DeBin, SerBin};

use naia_derive::Actor;
use naia_shared::{Actor, Property};

use crate::ExampleActor;

#[derive(Clone, PartialEq, DeBin, SerBin)]
pub enum PointActorColor {
    Red,
    Blue,
    Yellow,
}

impl Default for PointActorColor {
    fn default() -> Self {
        PointActorColor::Red
    }
}

#[derive(Actor)]
#[type_name = "ExampleActor"]
pub struct PointActor {
    pub x: Property<u16>,
    pub y: Property<u16>,
    pub color: Property<PointActorColor>,
}

impl PointActor {
    pub fn new(x: u16, y: u16, color: PointActorColor) -> PointActor {
        return PointActor::new_complete(x, y, color);
    }
}
