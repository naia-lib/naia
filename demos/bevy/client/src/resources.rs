use bevy::{asset::Handle, sprite::ColorMaterial};

use naia_bevy_demo_shared::protocol::KeyCommand;

pub struct Global {
    pub materials: Materials,
    pub queued_command: Option<KeyCommand>,
}

pub struct Materials {
    pub white: Handle<ColorMaterial>,
    pub red: Handle<ColorMaterial>,
    pub blue: Handle<ColorMaterial>,
    pub yellow: Handle<ColorMaterial>,
}
