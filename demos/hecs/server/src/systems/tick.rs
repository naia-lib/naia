use hecs::Entity;

use naia_hecs_server::shared::Random;

use naia_hecs_demo_shared::protocol::{Marker, Name, Position};

use crate::app::App;

pub fn march_and_mark(app: &mut App) {
    if !app.has_user {
        return;
    }
    // march entities across the screen
    let mut entities_to_add: Vec<Entity> = Vec::new();
    let mut entities_to_remove: Vec<Entity> = Vec::new();
    let mut entities_to_delete: Vec<Entity> = Vec::new();
    let mut entities_to_respawn: Vec<Entity> = Vec::new();

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

        if Random::gen_range_u32(0, 200) < 2 {
            entities_to_respawn.push(entity);
        }
    }

    // add markers
    while let Some(entity) = entities_to_add.pop() {
        if !app.has_marker.contains(&entity) {
            // Create Marker component
            let marker = Marker::default();

            // Add to Naia Server
            app.server
                .entity_mut(&mut app.world, &entity)
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
                .entity_mut(&mut app.world, &entity)
                .remove_component::<Marker>();
        }
    }

    while let Some(entity) = entities_to_delete.pop() {
        app.server.entity_mut(&mut app.world, &entity).despawn();
    }

    while let Some(entity) = entities_to_respawn.pop() {
        let first;
        let last;
        {
            let entity_ref = app.server.entity(&app.world, &entity);
            let old_name = entity_ref.component::<Name>().unwrap();
            first = (*old_name.full).first.clone();
            last = (*old_name.full).last.clone();
        }

        app.server.entity_mut(&mut app.world, &entity).despawn();

        let position_ref = Position::new(0, 0);

        // Create Name component
        let name_ref = Name::new(&first, &last);

        // Create an Entity
        app.server
            .spawn_entity(&mut app.world)
            .enter_room(&app.main_room_key)
            .insert_component(position_ref)
            .insert_component(name_ref)
            .id();
    }
}

pub fn check_scopes(app: &mut App) {
    // Update scopes of entities
    let server = &mut app.server;
    let world = &app.world;
    for (_, user_key, entity) in server.scope_checks() {
        if let Ok(entity_ref) = world.entity(entity) {
            if let Some(position) = entity_ref.get::<&Position>() {
                let x = *position.x;

                if (50..=200).contains(&x) {
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
    app.server.send_all_updates(&app.world);
}
