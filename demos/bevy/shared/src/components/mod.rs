use naia_bevy_shared::{Protocol, ProtocolPlugin};

mod color;
mod position;

pub use color::Color;
pub use position::Position;

// Plugin
pub struct ComponentsPlugin;

impl ProtocolPlugin for ComponentsPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol.add_component::<Color>().add_component::<Position>();
    }
}
