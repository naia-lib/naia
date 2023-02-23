use std::{thread::sleep, time::Duration};

use bevy_ecs::{
    entity::Entity,
    event::Events,
    schedule::ShouldRun,
    system::{Res, ResMut},
    world::{Mut, World},
};

use naia_server::Server;

mod naia_events {
    pub use naia_server::{
        AuthEvent, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvent, TickEvent,
    };
}

mod bevy_events {
    pub use crate::events::{AuthEvents, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvents, TickEvent};
}

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<Entity>>| {
        let mut events = server.receive();
        unsafe {
            // Connect Event
            let mut connect_event_writer = world
                .get_resource_unchecked_mut::<Events<bevy_events::ConnectEvent>>()
                .unwrap();
            for user_key in events.read::<naia_events::ConnectEvent>() {
                connect_event_writer.send(bevy_events::ConnectEvent(user_key));
            }

            // Disconnect Event
            let mut disconnect_event_writer = world
                .get_resource_unchecked_mut::<Events<bevy_events::DisconnectEvent>>()
                .unwrap();
            for (user_key, user) in events.read::<naia_events::DisconnectEvent>() {
                disconnect_event_writer.send(bevy_events::DisconnectEvent(user_key, user));
            }

            // Error Event
            let mut error_event_writer = world
                .get_resource_unchecked_mut::<Events<bevy_events::ErrorEvent>>()
                .unwrap();
            for error in events.read::<naia_events::ErrorEvent>() {
                error_event_writer.send(bevy_events::ErrorEvent(error));
            }

            // Tick Event
            let mut tick_event_writer = world
                .get_resource_unchecked_mut::<Events<bevy_events::TickEvent>>()
                .unwrap();
            for tick in events.read::<naia_events::TickEvent>() {
                tick_event_writer.send(bevy_events::TickEvent(tick));
            }

            // Message Event
            let mut message_event_writer = world
                .get_resource_unchecked_mut::<Events<bevy_events::MessageEvents>>()
                .unwrap();
            message_event_writer.send(bevy_events::MessageEvents::from(&mut events));

            // Auth Event
            let mut auth_event_writer = world
                .get_resource_unchecked_mut::<Events<bevy_events::AuthEvents>>()
                .unwrap();
            auth_event_writer.send(bevy_events::AuthEvents::from(&mut events));
        }

        sleep(server.duration_until_next_tick());
    });
}

pub fn should_receive(server: Res<Server<Entity>>) -> ShouldRun {
    if server.is_listening() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}
