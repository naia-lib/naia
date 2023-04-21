use bevy::prelude::{Quat, Query, Transform, Vec2, With};

use naia_bevy_client::Client;
use naia_bevy_demo_shared::components::{Baseline, Position};

use crate::components::{Confirmed, Interp, Line, LocalCursor, Predicted};

pub fn sync_clientside_sprites(
    client: Client,
    mut query: Query<(&Position, &mut Interp, &mut Transform), With<Predicted>>,
) {
    for (position, mut interp, mut transform) in query.iter_mut() {
        if *position.x != interp.next_x as i16 || *position.y != interp.next_y as i16 {
            interp.next_position(*position.x, *position.y);
        }

        let interp_amount = client.client_interpolation().unwrap();
        interp.interpolate(interp_amount);
        transform.translation.x = interp.interp_x;
        transform.translation.y = interp.interp_y;
    }
}

pub fn sync_serverside_sprites(
    client: Client,
    mut query: Query<(&Position, &mut Interp, &mut Transform), With<Confirmed>>,
) {
    for (position, mut interp, mut transform) in query.iter_mut() {
        if *position.x != interp.next_x as i16 || *position.y != interp.next_y as i16 {
            interp.next_position(*position.x, *position.y);
        }

        let interp_amount = client.server_interpolation().unwrap();
        interp.interpolate(interp_amount);
        transform.translation.x = interp.interp_x;
        transform.translation.y = interp.interp_y;
    }
}

pub fn sync_cursor_sprite(mut query: Query<(&Position, &mut Transform), With<LocalCursor>>) {
    for (position, mut transform) in query.iter_mut() {
        transform.translation.x = *position.x as f32;
        transform.translation.y = *position.y as f32;
    }
}

pub fn sync_baseline(
    mut query: Query<(&Baseline, &mut Transform), With<Confirmed>>,
) {
    for (baseline, mut transform) in query.iter_mut() {
        transform.translation.x = *baseline.x as f32;
        transform.translation.y = *baseline.y as f32;
    }
}

pub fn sync_relation_lines(
    position_query: Query<&Position>,
    baseline_query: Query<&Baseline>,
    mut line_query: Query<(&mut Transform, &Line)>
) {
    for (mut line_transform, line_entities) in line_query.iter_mut() {
        if let Ok(start) = position_query.get(line_entities.start_entity) {
            if let Ok(end) = baseline_query.get(line_entities.end_entity) {
                let start_vec2 = Vec2::new(*start.x as f32, *start.y as f32);
                let end_vec2 = Vec2::new(*end.x as f32, *end.y as f32);
                line_transform.translation.x = start_vec2.x;
                line_transform.translation.y = start_vec2.y;
                line_transform.scale.x = start_vec2.distance(end_vec2);
                let angle = {
                    let dx = end_vec2.x - start_vec2.x;
                    let dy = end_vec2.y - start_vec2.y;
                    dy.atan2(dx)
                };
                line_transform.rotation = Quat::from_rotation_z(angle);
            }
        }
    }
}