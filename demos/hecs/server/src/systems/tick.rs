use naia_hecs_server::{Entity, Ref, WorldProxy, WorldProxyMut};

use naia_hecs_demo_shared::protocol::{Marker, Position, StringMessage};

use crate::app::App;

pub fn march_and_mark(app: &mut App) {
    // march entities across the screen
    let mut entities_to_add: Vec<Entity> = Vec::new();
    let mut entities_to_remove: Vec<Entity> = Vec::new();

    for (entity_key, position_ref) in app.world.query_mut::<&Ref<Position>>() {
        let mut position = position_ref.borrow_mut();
        let mut x = *position.x.get();
        x += 1;
        if x > 125 {
            x = 0;
            let mut y = *position.y.get();
            y = y.wrapping_add(1);
            position.y.set(y);
        }
        if x == 40 {
            entities_to_add.push(Entity::new(entity_key));
        }
        if x == 75 {
            entities_to_remove.push(Entity::new(entity_key));
        }
        position.x.set(x);
    }

    // add markers
    while let Some(entity_key) = entities_to_add.pop() {
        if !app.has_marker.contains(&entity_key) {
            // Create Marker component
            let marker = Marker::new("new");

            // Add to Naia Server
            app.server
                .entity_mut(&mut app.world.proxy_mut(), &entity_key)
                .insert_component(&marker);

            // Track that this entity has a Marker
            app.has_marker.insert(entity_key);
        }
    }

    // remove markers
    while let Some(entity_key) = entities_to_remove.pop() {
        if app.has_marker.remove(&entity_key) {
            // Remove from Naia Server
            app.server
                .entity_mut(&mut app.world.proxy_mut(), &entity_key)
                .remove_component::<Marker>();
        }
    }
}

pub fn send_messages(app: &mut App) {
    // Message Sending
    for user_key in app.server.user_keys() {
        let address = app.server.user(&user_key).address();
        let message_contents = format!("Server Packet (tick {})", app.tick_count);
        info!("Naia Server send -> {}: {}", address, message_contents);

        let message = StringMessage::new(message_contents);
        app.server.queue_message(&user_key, &message, true);
    }

    app.tick_count = app.tick_count.wrapping_add(1);
}

pub fn check_scopes(app: &mut App) {
    // Update scopes of entities
    for (_, user_key, entity_key) in app.server.scope_checks() {
        if let Some(pos_ref) = app
            .server
            .entity(app.world.proxy(), &entity_key)
            .component::<Position>()
        {
            let x = *pos_ref.borrow().x.get();
            if x >= 5 && x <= 100 {
                app.server.user_scope(&user_key).include(&entity_key);
            } else {
                app.server.user_scope(&user_key).exclude(&entity_key);
            }
        }
    }
}

pub fn send_updates(app: &mut App) {
    // VERY IMPORTANT! Calling this actually sends all update data
    // packets to all Clients that require it. If you don't call this
    // method, the Server will never communicate with it's connected Clients
    app.server.send_all_updates(app.world.proxy());
}
