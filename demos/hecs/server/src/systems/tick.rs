use hecs::Entity;

use naia_hecs_server::{WorldProxy, WorldProxyMut};

use naia_hecs_demo_shared::protocol::{Marker, Position};

use crate::app::App;

pub fn march_and_mark(app: &mut App) {
    if !app.has_user {
        return;
    }
    // march entities across the screen
    let mut entities_to_add: Vec<Entity> = Vec::new();
    let mut entities_to_remove: Vec<Entity> = Vec::new();
    let mut entities_to_delete: Vec<Entity> = Vec::new();

    for (entity, position) in app.world.query_mut::<&mut Position>() {

        *position.x += 1;

        if *position.x == 100 {
            entities_to_add.push(entity);
        }
        if *position.x == 150 {
            entities_to_remove.push(entity);
        }
        if *position.x > 250 {
            *position.x = 0;
            if *position.y == 3 {
                entities_to_delete.push(entity);
            }
            *position.y += 1;

        }
    }

    // add markers
    while let Some(entity) = entities_to_add.pop() {
        if !app.has_marker.contains(&entity) {
            // Create Marker component
            let marker = Marker::new("new");

            // Add to Naia Server
            app.server
                .entity_mut(app.world.proxy_mut(&mut app.world_data), &entity)
                .insert_component(marker);

            // Track that this entity has a Marker
            app.has_marker.insert(entity);
        }
    }

    // remove markers
    while let Some(entity) = entities_to_remove.pop() {
        if app.has_marker.remove(&entity) {
            // Remove from Naia Server
            app.server
                .entity_mut(app.world.proxy_mut(&mut app.world_data), &entity)
                .remove_component::<Marker>();
        }
    }

    while let Some(entity) = entities_to_delete.pop() {
        app.server.entity_mut(app.world.proxy_mut(&mut app.world_data), &entity).despawn();
    }
}

pub fn check_scopes(app: &mut App) {
    // Update scopes of entities
    let server = &mut app.server;
    let world = &app.world;
    for (_, user_key, entity) in server.scope_checks() {
        if let Ok(entity_ref) = world.entity(entity) {
            if let Some(position) = entity_ref.get::<Position>() {
                let x = *position.x;

                if x >= 50 && x <= 200 {
                    server.user_scope(&user_key).include(&entity);
                } else {
                    server.user_scope(&user_key).exclude(&entity);
                }
            }
        }
    }
}

pub fn send_updates(app: &mut App) {
    // VERY IMPORTANT! Calling this actually sends all update data
    // packets to all Clients that require it. If you don't call this
    // method, the Server will never communicate with it's connected Clients
    app.server
        .send_all_updates(app.world.proxy(&app.world_data));
}
