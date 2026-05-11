use std::collections::HashSet;

use bevy::prelude::{ColorMaterial, Entity, Handle, Mesh, Resource};

use naia_bevy_client::{CommandHistory, ResponseReceiveKey};
use naia_bevy_demo_shared::messages::{BasicResponse, KeyCommand};

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
    // 128 ticks covers ~6s at 20 Hz — sufficient prediction window for any realistic RTT
    pub command_history: CommandHistory<KeyCommand>,
    pub red: Handle<ColorMaterial>,
    pub blue: Handle<ColorMaterial>,
    pub yellow: Handle<ColorMaterial>,
    pub green: Handle<ColorMaterial>,
    pub white: Handle<ColorMaterial>,
    pub purple: Handle<ColorMaterial>,
    pub orange: Handle<ColorMaterial>,
    pub aqua: Handle<ColorMaterial>,
    pub circle: Handle<Mesh>,
    pub response_keys: HashSet<ResponseReceiveKey<BasicResponse>>,
    pub request_index: u8,
}

impl Default for Global {
    fn default() -> Self {
        Self {
            owned_entity: None,
            cursor_entity: None,
            queued_command: None,
            command_history: CommandHistory::new(128),
            red: Default::default(),
            blue: Default::default(),
            yellow: Default::default(),
            green: Default::default(),
            white: Default::default(),
            purple: Default::default(),
            orange: Default::default(),
            aqua: Default::default(),
            circle: Default::default(),
            response_keys: HashSet::new(),
            request_index: 0,
        }
    }
}

