use std::{thread::sleep, time::Duration};

use naia_hecs_demo_shared::Auth;
use naia_hecs_server::{AuthEvent, ConnectEvent, DisconnectEvent, ErrorEvent, TickEvent};

use crate::app::App;

pub fn process_events(app: &mut App) {
    let mut events = app.server.receive(&mut app.world);
    if events.is_empty() {
        // If we don't sleep here, app will loop at 100% CPU until a new message comes in
        sleep(Duration::from_millis(3));
        return;
    } else {
        for (user_key, auth) in events.read::<AuthEvent<Auth>>() {
            if auth.username == "charlie" && auth.password == "12345" {
                // Accept incoming connection
                app.server.accept_connection(&user_key);
            } else {
                // Reject incoming connection
                app.server.reject_connection(&user_key);
            }
        }
        for user_key in events.read::<ConnectEvent>() {
            let address = app
                .server
                .user_mut(&user_key)
                .enter_room(&app.main_room_key)
                .address();
            info!("Naia Server connected to: {}", address);
            app.has_user = true;
        }
        for (_user_key, user) in events.read::<DisconnectEvent>() {
            info!("Naia Server disconnected from: {:?}", user.address);
        }
        for _ in events.read::<TickEvent>() {
            app.tick();
        }
        for error in events.read::<ErrorEvent>() {
            info!("Naia Server Error: {}", error);
        }
    }
}
