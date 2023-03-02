use bevy_ecs::system::{Query, Res, ResMut};
use bevy_input::{keyboard::KeyCode, Input};

use naia_bevy_client::Client;
use naia_bevy_demo_shared::components::Position;

use naia_bevy_demo_shared::messages::KeyCommand;

use crate::resources::Global;

pub fn server_input(mut global: ResMut<Global>, client: Client, keyboard_input: Res<Input<KeyCode>>) {
    let w = keyboard_input.pressed(KeyCode::W);
    let s = keyboard_input.pressed(KeyCode::S);
    let a = keyboard_input.pressed(KeyCode::A);
    let d = keyboard_input.pressed(KeyCode::D);

    if let Some(command) = &mut global.queued_command {
        if w {
            command.w = true;
        }
        if s {
            command.s = true;
        }
        if a {
            command.a = true;
        }
        if d {
            command.d = true;
        }
    } else if let Some(owned_entity) = &global.owned_entity {
        let mut key_command = KeyCommand::new(w, s, a, d);
        key_command.entity.set(&client, &owned_entity.confirmed);
        global.queued_command = Some(key_command);
    }
}

pub fn client_input(global: ResMut<Global>, keyboard_input: Res<Input<KeyCode>>, mut query: Query<&mut Position>) {
    if let Some(entity) = global.client_authoritative_entity {
        let i = keyboard_input.pressed(KeyCode::I);
        let k = keyboard_input.pressed(KeyCode::K);
        let j = keyboard_input.pressed(KeyCode::J);
        let l = keyboard_input.pressed(KeyCode::L);

        if let Ok(mut position) = query.get_mut(entity) {
            if i {
                *position.y = position.y.wrapping_sub(2);
            }
            if k {
                *position.y = position.y.wrapping_add(2);
            }
            if j {
                *position.x = position.x.wrapping_sub(2);
            }
            if l {
                *position.x = position.x.wrapping_add(2);
            }
        }
    }
}

//pub fn sync(mut query: Query<(&Position, &mut Transform)>) {
//     for (pos, mut transform) in query.iter_mut() {
//         transform.translation.x = f32::from(*pos.x);
//         transform.translation.y = f32::from(*pos.y) * -1.0;
//     }
// }