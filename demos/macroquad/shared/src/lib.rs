extern crate log;
extern crate naia_shared;

pub mod behavior;
pub mod components;
pub mod messages;

pub mod channels;
mod protocol;

pub use protocol::protocol;