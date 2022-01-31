use bevy::{
    app::Events,
    ecs::{
        entity::Entity,
        schedule::ShouldRun,
        system::{Res, ResMut},
        world::{Mut, World},
    },
};
use naia_server::{Event, Protocolize, Server};

use super::{
    events::{AuthorizationEvent, CommandEvent, ConnectionEvent, DisconnectionEvent, MessageEvent},
    resource::ServerResource,
};

pub fn before_receive_events<P: Protocolize>(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<P, Entity>>| {
        world.resource_scope(|world, mut server_resource: Mut<ServerResource>| {
            let event_results = server.receive();

            unsafe {
                let mut authorize_event_writer = world
                    .get_resource_unchecked_mut::<Events<AuthorizationEvent<P>>>()
                    .unwrap();
                let mut connect_event_writer = world
                    .get_resource_unchecked_mut::<Events<ConnectionEvent>>()
                    .unwrap();
                let mut disconnect_event_writer = world
                    .get_resource_unchecked_mut::<Events<DisconnectionEvent>>()
                    .unwrap();
                let mut message_event_writer = world
                    .get_resource_unchecked_mut::<Events<MessageEvent<P>>>()
                    .unwrap();
                let mut command_event_writer = world
                    .get_resource_unchecked_mut::<Events<CommandEvent<P>>>()
                    .unwrap();

                for event_result in event_results {
                    match event_result {
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
                        Ok(Event::Message(user_key, message)) => {
                            message_event_writer.send(MessageEvent(user_key, message));
                        }
                        Ok(Event::Command(user_key, entity, command)) => {
                            command_event_writer.send(CommandEvent(user_key, entity, command));
                        }
                        Err(_) => {}
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

pub fn should_receive<P: Protocolize>(server: Res<Server<P, Entity>>) -> ShouldRun {
    if server.is_listening() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}
