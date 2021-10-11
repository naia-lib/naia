use bevy::ecs::world::{Mut, World};

use naia_client::{Event, ProtocolType, Client};

use naia_bevy_shared::{Entity, WorldProxyMut, tick::Ticker};

use super::resource::ClientResource;

pub fn before_receive_events<P: ProtocolType>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<P, Entity>>| {
        world.resource_scope(|world, mut client_resource: Mut<ClientResource<P>>| {
            world.resource_scope(|world, mut ticker: Mut<Ticker>| {
                for event_result in client.receive(&mut world.proxy_mut()) {
                    match event_result {
                        Ok(Event::Tick) => {
                            ticker.tick_start();
                        }
                        event => {
                            client_resource.push_event(event);
                        }
                    }
                }
            });
        });
    });
}