use bevy::prelude::*;

use naia_bevy_demo_shared::protocol::KeyCommand;

use crate::resources::Global;

pub fn input(keyboard_input: Res<Input<KeyCode>>, mut global: ResMut<Global>) {
    let w = keyboard_input.pressed(KeyCode::W);
    let s = keyboard_input.pressed(KeyCode::S);
    let a = keyboard_input.pressed(KeyCode::A);
    let d = keyboard_input.pressed(KeyCode::D);

    if let Some(command) = &mut global.queued_command {
        if w {
            *command.w = true;
        }
        if s {
            *command.s = true;
        }
        if a {
            *command.a = true;
        }
        if d {
            *command.d = true;
        }
    } else {
        global.queued_command = Some(KeyCommand::new(w, s, a, d));
    }
}
