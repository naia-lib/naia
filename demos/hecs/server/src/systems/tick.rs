use hecs::Entity;

use naia_hecs_server::{WorldProxy, WorldProxyMut};

use naia_hecs_demo_shared::protocol::{Marker, Position};

use crate::app::App;

pub fn march_and_mark(app: &mut App) {
    // march entities across the screen
    let mut entities_to_add: Vec<Entity> = Vec::new();
    let mut entities_to_remove: Vec<Entity> = Vec::new();

    for (entity, position) in app.world.query_mut::<&mut Position>() {
        let mut x = *position.x.get();
        x += 1;
        if x > 125 {
            x = 0;
            let mut y = *position.y.get();
            y = y.wrapping_add(1);
            position.y.set(y);
        }
        if x == 40 {
            entities_to_add.push(entity);
        }
        if x == 75 {
            entities_to_remove.push(entity);
        }
        position.x.set(x);
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
}

pub fn check_scopes(app: &mut App) {
    // Update scopes of entities
    let server = &mut app.server;
    let world = &app.world;
    for (_, user_key, entity) in server.scope_checks() {
        if let Ok(entity_ref) = world.entity(entity) {
            if let Some(position) = entity_ref.get::<Position>() {
                let x = *position.x.get();

                if x >= 5 && x <= 100 {
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
