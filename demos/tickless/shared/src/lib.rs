extern crate log;
extern crate naia_derive;

mod protocol;
mod shared;
mod text;

pub use protocol::Protocol;
pub use shared::get_shared_config;
pub use text::Text;
