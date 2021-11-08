#[macro_use]
extern crate slotmap;

pub use naia_shared::{WorldMutType, WorldRefType};

mod component_ref;
mod entity;
mod world;

pub use entity::Entity;
pub use world::World;
