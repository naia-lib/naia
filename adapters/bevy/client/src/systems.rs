use bevy::{
    ecs::{
        world::{Mut, World},
    },
    log::info,
};

use naia_client::{Event, ProtocolType, Client};

use naia_bevy_shared::{Entity, WorldProxyMut};

use super::resource::ClientResource;

pub fn receive_events<P: ProtocolType>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<P, Entity>>| {
        world.resource_scope(|world, mut client_resource: Mut<ClientResource>| {
            for event_result in client.receive(&mut world.proxy_mut()) {
                match event_result {
                    Ok(Event::Tick) => {
                        client_resource.tick_start();
                    }
                    Err(error) => {
                        info!("Naia Server error: {}", error);
                    }
                    Ok(event) => {
                        client_resource.push_event(event);
                    }
                }
            }
        });
    });
}