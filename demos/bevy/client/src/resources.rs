use std::default::Default;

use bevy_asset::Handle;
use bevy_ecs::{entity::Entity, prelude::Resource};
use bevy_render::mesh::Mesh;
use bevy_sprite::ColorMaterial;

use naia_bevy_client::CommandHistory;
use naia_bevy_demo_shared::messages::KeyCommand;

pub struct OwnedEntity {
    pub confirmed: Entity,
    pub predicted: Entity,
}

impl OwnedEntity {
    pub fn new(confirmed_entity: Entity, predicted_entity: Entity) -> Self {
        OwnedEntity {
            confirmed: confirmed_entity,
            predicted: predicted_entity,
        }
    }
}

#[derive(Resource)]
pub struct Global {
    pub owned_entity: Option<OwnedEntity>,
    pub cursor_entity: Option<Entity>,
    pub queued_command: Option<KeyCommand>,
    pub command_history: CommandHistory<KeyCommand>,
    pub red: Handle<ColorMaterial>,
    pub blue: Handle<ColorMaterial>,
    pub yellow: Handle<ColorMaterial>,
    pub green: Handle<ColorMaterial>,
    pub white: Handle<ColorMaterial>,
    pub circle: Handle<Mesh>,
}

impl Default for Global {
    fn default() -> Self {
        Self {
            owned_entity: None,
            cursor_entity: None,
            queued_command: None,
            command_history: CommandHistory::default(),
            white: Handle::default(),
            red: Handle::default(),
            blue: Handle::default(),
            yellow: Handle::default(),
            green: Handle::default(),
            circle: Handle::default(),
        }
    }
}
