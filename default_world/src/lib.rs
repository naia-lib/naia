#[macro_use]
extern crate slotmap;

#[macro_use]
extern crate cfg_if;

pub use naia_shared::{WorldMutType, WorldRefType};

mod world;

pub use world::{Entity, World};

cfg_if! {
    if #[cfg(all(feature = "client", feature = "server"))]
    {
        // Use both protocols...
        compile_error!("naia-shared requires either the 'client' OR 'server' feature to be enabled, you must pick one.");
    }
    else if #[cfg(all(not(feature = "client"), not(feature = "server")))]
    {
        // Use no protocols...
        compile_error!("naia-shared requires either the 'client' OR 'server' feature to be enabled, you must pick one.");
    }
}
