use naia_shared::{Protocol, ProtocolPlugin};

mod marker;
mod square;

pub use marker::Marker;
pub use square::{Color, Square};

// Plugin
pub struct ComponentsPlugin;

impl ProtocolPlugin for ComponentsPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol.add_component::<Square>().add_component::<Marker>();
    }
}
