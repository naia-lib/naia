use naia_bevy_shared::{Protocol, ProtocolPlugin};

mod color;
pub use color::{Color, ColorValue};

mod position;
pub use position::Position;

mod shape;
pub use shape::{Shape, ShapeValue};

mod relation;
pub use relation::Relation;

// Plugin
pub struct ComponentsPlugin;

impl ProtocolPlugin for ComponentsPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol
            .add_component::<Color>()
            .add_component::<Position>()
            .add_component::<Shape>()
            .add_component::<Relation>();
    }
}
