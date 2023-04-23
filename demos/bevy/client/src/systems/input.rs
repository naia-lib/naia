use bevy::prelude::{Input, KeyCode, Query, Res, ResMut, Vec2, Window};

use naia_bevy_client::Client;
use naia_bevy_demo_shared::{components::Position, messages::KeyCommand};

use crate::resources::Global;

pub fn key_input(mut global: ResMut<Global>, client: Client, keyboard_input: Res<Input<KeyCode>>) {
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

pub fn cursor_input(
    global: ResMut<Global>,
    window_query: Query<&Window>,
    mut position_query: Query<&mut Position>,
) {
    if let Some(entity) = global.cursor_entity {
        if let Ok(window) = window_query.get_single() {
            if let Ok(mut cursor_position) = position_query.get_mut(entity) {
                if let Some(mouse_position) = window_relative_mouse_position(window) {
                    *cursor_position.x = mouse_position.x as i16;
                    *cursor_position.y = mouse_position.y as i16;
                }
            }
        }
    }
}

fn window_relative_mouse_position(window: &Window) -> Option<Vec2> {
    let Some(cursor_pos) = window.cursor_position() else {return None};

    let window_size = Vec2 {
        x: window.width(),
        y: window.height(),
    };

    Some(cursor_pos - window_size / 2.0)
}
