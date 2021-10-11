use bevy::prelude::*;

use naia_bevy_demo_shared::protocol::KeyCommand;

use crate::resources::Global;

pub fn player_input(keyboard_input: Res<Input<KeyCode>>, mut global: ResMut<Global>) {
    let w = keyboard_input.pressed(KeyCode::W);
    let s = keyboard_input.pressed(KeyCode::S);
    let a = keyboard_input.pressed(KeyCode::A);
    let d = keyboard_input.pressed(KeyCode::D);

    if let Some(command_ref) = &mut global.queued_command {
        let mut command = command_ref.borrow_mut();
        if w {
            command.w.set(true);
        }
        if s {
            command.s.set(true);
        }
        if a {
            command.a.set(true);
        }
        if d {
            command.d.set(true);
        }
    } else {
        global.queued_command = Some(KeyCommand::new(w, s, a, d));
    }
}
