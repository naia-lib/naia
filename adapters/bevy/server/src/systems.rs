use bevy_ecs::{
    entity::Entity,
    event::Events,
    schedule::ShouldRun,
    system::{Res, ResMut},
    world::{Mut, World},
};
use naia_server::{
    Event, Server,
};

use super::{
    events::{AuthorizationEvent, ConnectionEvent, DisconnectionEvent, MessageEvent},
    resource::ServerResource,
};

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<Entity>>| {
        world.resource_scope(|world, mut server_resource: Mut<ServerResource>| {
            let events = server.receive();
            if events.is_empty() {
                // In the future, may want to stall the system if we don't receive any events
                // to keep from the system running empty and using up CPU.
            } else {
                unsafe {
                    let mut authorize_event_writer = world
                        .get_resource_unchecked_mut::<Events<AuthorizationEvents>>()
                        .unwrap();
                    let mut connect_event_writer = world
                        .get_resource_unchecked_mut::<Events<ConnectionEvent>>()
                        .unwrap();
                    let mut disconnect_event_writer = world
                        .get_resource_unchecked_mut::<Events<DisconnectionEvent>>()
                        .unwrap();
                    let mut message_event_writer = world
                        .get_resource_unchecked_mut::<Events<MessageEvents>>()
                        .unwrap();

                    for event in events {
                        match event {
                            Ok(Event::Tick) => {
                                server_resource.ticker.set();
                                continue;
                            }
                            Ok(Event::Authorization(user_key, auth)) => {
                                authorize_event_writer.send(AuthorizationEvent(user_key, auth));
                            }
                            Ok(Event::Connection(user_key)) => {
                                connect_event_writer.send(ConnectionEvent(user_key));
                            }
                            Ok(Event::Disconnection(user_key, user)) => {
                                disconnect_event_writer.send(DisconnectionEvent(user_key, user));
                            }
                            Ok(Event::Message(user_key, channel, message)) => {
                                message_event_writer.send(MessageEvent(user_key, channel, message));
                            }
                            Err(_) => {}
                        }
                    }
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

pub fn should_receive(
    server: Res<Server<Entity>>,
) -> ShouldRun {
    if server.is_listening() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}
