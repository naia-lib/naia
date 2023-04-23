use bevy::prelude::{Query, Transform, With};

use naia_bevy_client::Client;
use naia_bevy_demo_shared::components::Position;

use crate::components::{Confirmed, Interp, LocalCursor, Predicted};

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
