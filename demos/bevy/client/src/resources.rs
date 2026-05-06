use std::{collections::HashSet, default::Default};

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
#[derive(Default)]
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
    pub purple: Handle<ColorMaterial>,
    pub orange: Handle<ColorMaterial>,
    pub aqua: Handle<ColorMaterial>,
    pub circle: Handle<Mesh>,
    pub response_keys: HashSet<ResponseReceiveKey<BasicResponse>>,
    pub request_index: u8,
}

