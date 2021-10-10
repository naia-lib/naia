use bevy::ecs::{
    schedule::ShouldRun,
    system::Res,
    world::{Mut, World},
};

use naia_server::{ProtocolType, Server};

use crate::world::{entity::Entity, world_proxy::WorldProxy};

use super::resource::ServerResource;

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
