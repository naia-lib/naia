use bevy::{transform::components::Transform, ecs::system::Query};

use naia_bevy_client::{Ref, components::{Confirmed, Predicted}};

use naia_bevy_demo_shared::protocol::Position;

pub fn predicted_sync(mut query: Query<(&Predicted, &Ref<Position>, &mut Transform)>) {
    for (_, pos_ref, mut transform) in query.iter_mut() {
        let pos = pos_ref.borrow();
        transform.translation.x = f32::from(*(pos.x.get()));
        transform.translation.y = f32::from(*(pos.y.get())) * -1.0;
    }
}

pub fn confirmed_sync(mut query: Query<(&Confirmed, &Ref<Position>, &mut Transform)>) {
    for (_, pos_ref, mut transform) in query.iter_mut() {
        let pos = pos_ref.borrow();
        transform.translation.x = f32::from(*(pos.x.get()));
        transform.translation.y = f32::from(*(pos.y.get())) * -1.0;
    }
}