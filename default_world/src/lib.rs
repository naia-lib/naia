#[macro_use]
extern crate slotmap;

pub use naia_shared::{WorldRefType, WorldMutType};

mod world;

pub use world::{Entity, World};
