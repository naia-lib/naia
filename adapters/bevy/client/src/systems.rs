use bevy::ecs::{
    world::{Mut, World},
    schedule::ShouldRun,
    system::{Res, ResMut},
};

use naia_client::{Client, Event, ProtocolType};

use naia_bevy_shared::{Entity, WorldProxyMut};

use super::{
    components::{Confirmed, Predicted},
    resource::ClientResource,
};

pub fn before_receive_events<P: ProtocolType>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<P, Entity>>| {
        world.resource_scope(|world, mut client_resource: Mut<ClientResource<P>>| {
                for event_result in client.receive(&mut world.proxy_mut()) {
                    match event_result {
                        Ok(Event::Tick) => {
                            client_resource.ticker.set();
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
}

pub fn should_connect<P: ProtocolType>(resource: Res<ClientResource<P>>) -> ShouldRun {
    if resource.connector.is_set() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn finish_connect<P: ProtocolType>(mut resource: ResMut<ClientResource<P>>) {
    resource.connector.reset();
}

pub fn should_disconnect<P: ProtocolType>(resource: Res<ClientResource<P>>) -> ShouldRun {
    if resource.connector.is_set() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn finish_disconnect<P: ProtocolType>(mut resource: ResMut<ClientResource<P>>) {
    resource.connector.reset();
}

pub fn should_tick<P: ProtocolType>(resource: Res<ClientResource<P>>) -> ShouldRun {
    if resource.ticker.is_set() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn finish_tick<P: ProtocolType>(mut resource: ResMut<ClientResource<P>>) {
    resource.ticker.reset();
}