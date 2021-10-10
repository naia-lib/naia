use bevy::{
    app::Events,
    ecs::{
        schedule::ShouldRun,
        system::Res,
        world::{Mut, World},
    },
    log::info,
};

use naia_server::{Event, ProtocolType, Server};

use crate::world::{entity::Entity, world_proxy::WorldProxy};

use super::plugin::ServerResource;

pub fn read_server_events<P: ProtocolType>(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<P, Entity>>| {
        world.resource_scope(|world, mut server_resource: Mut<ServerResource>| {
            world.resource_scope(|world, mut server_events: Mut<Events<Event<P, Entity>>>| {
                for event_result in server.receive(world.proxy()) {
                    match event_result {
                        Ok(Event::Tick) => {
                            server_resource.ticked = true;
                        }
                        Err(error) => {
                            info!("Naia Server error: {}", error);
                        }
                        Ok(event) => {
                            server_events.send(event);
                        }
                    }
                }
            });
        });
    });
}

pub fn should_tick(server_resource: Res<ServerResource>) -> ShouldRun {
    if server_resource.ticked {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn send_server_packets<P: ProtocolType>(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<P, Entity>>| {
        // VERY IMPORTANT! Calling this actually sends all update data
        // packets to all Clients that require it. If you don't call this
        // method, the Server will never communicate with it's connected Clients
        server.send_all_updates(world.proxy());
    });

    if let Some(mut server_resource) = world.get_resource_mut::<ServerResource>() {
        server_resource.ticked = false;
    }
}
