use naia_bevy_shared::{Protocol, ProtocolPlugin};

mod color;
pub use color::{Color, ColorValue};

mod position;
pub use position::Position;

mod shape;
pub use shape::{Shape, ShapeValue};

// Plugin
pub struct ComponentsPlugin;

impl ProtocolPlugin for ComponentsPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol
            .register_component::<Color>()
            .register_component::<Position>()
            .register_component::<Shape>();
    }
}
