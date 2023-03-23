use naia_shared::{Protocol, ProtocolPlugin};

mod color;
mod position;
mod shape;

pub use color::{Color, ColorValue};
pub use position::Position;
pub use shape::{Shape, ShapeValue};

// Plugin
pub struct ComponentsPlugin;

impl ProtocolPlugin for ComponentsPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol
            .add_component::<Color>()
            .add_component::<Position>()
            .add_component::<Shape>();
    }
}
