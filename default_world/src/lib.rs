#[macro_use]
extern crate slotmap;

pub use naia_shared::{WorldMutType, WorldRefType};

mod world;

pub use world::{Entity, World};