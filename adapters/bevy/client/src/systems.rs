use bevy::ecs::world::{Mut, World};

use naia_client::{Client, Event, ProtocolType};

use naia_bevy_shared::{tick::Ticker, Entity, IntoWorldMutType};

use super::{
    components::{Confirmed, Predicted},
    resource::ClientResource,
};

pub fn before_receive_events<P: ProtocolType>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<P, Entity>>| {
        world.resource_scope(|world, mut client_resource: Mut<ClientResource<P>>| {
            world.resource_scope(|world, mut ticker: Mut<Ticker>| {
                for event_result in client.receive(&mut world.proxy_mut()) {
                    match event_result {
                        Ok(Event::Tick) => {
                            ticker.tick_start();
                            continue;
                        }
                        Ok(Event::SpawnEntity(entity, _)) => {
                            world.entity_mut(*entity).insert(Confirmed);
                        }
                        Ok(Event::OwnEntity(ref owned_entity)) => {
                            let predicted_entity = owned_entity.predicted;
                            world.entity_mut(*predicted_entity).insert(Predicted);
                        }
                        _ => {}
                    }

                    client_resource.push_event(event_result);
                }
            });
        });
    });
}
