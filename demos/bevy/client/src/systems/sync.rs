use bevy::{
    ecs::{query::With, system::Query},
    transform::components::Transform,
};

use naia_bevy_client::{
    components::{Confirmed, Predicted},
    Ref,
};

use naia_bevy_demo_shared::protocol::Position;

pub fn predicted_sync(mut query: Query<(&Ref<Position>, &mut Transform), With<Predicted>>) {
    for (pos_ref, mut transform) in query.iter_mut() {
        sync_transform(pos_ref, &mut transform);
    }
}

pub fn confirmed_sync(mut query: Query<(&Ref<Position>, &mut Transform), With<Confirmed>>) {
    for (pos_ref, mut transform) in query.iter_mut() {
        sync_transform(pos_ref, &mut transform);
    }
}

fn sync_transform(pos_ref: &Ref<Position>, transform: &mut Transform) {
    let pos = pos_ref.borrow();
    transform.translation.x = f32::from(*(pos.x.get()));
    transform.translation.y = f32::from(*(pos.y.get())) * -1.0;
}
