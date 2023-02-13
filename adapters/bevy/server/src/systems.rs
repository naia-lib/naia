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
    pub use crate::events::{AuthEvents, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvents};
}

use super::resource::ServerResource;

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<Entity>>| {
        world.resource_scope(|world, mut server_resource: Mut<ServerResource>| {
            let mut events = server.receive();
            if events.is_empty() {
                // In the future, may want to stall the system if we don't receive any events
                // to keep from the system running empty and using up CPU.
                sleep(Duration::from_millis(5));
            } else {
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

                    // Tick Event
                    for _ in events.read::<naia_events::TickEvent>() {
                        server_resource.ticker.set();
                    }

                    // Error Event
                    let mut error_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::ErrorEvent>>()
                        .unwrap();
                    for error in events.read::<naia_events::ErrorEvent>() {
                        error_event_writer.send(bevy_events::ErrorEvent(error));
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
            }
        });
    });
}

pub fn should_tick(resource: Res<ServerResource>) -> ShouldRun {
    if resource.ticker.is_set() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

pub fn finish_tick(mut resource: ResMut<ServerResource>) {
    resource.ticker.reset();
}

pub fn should_receive(server: Res<Server<Entity>>) -> ShouldRun {
    if server.is_listening() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}
