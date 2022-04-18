use bevy_ecs::{
    entity::Entity,
    event::Events,
    schedule::ShouldRun,
    system::{Res, ResMut},
    world::{Mut, World},
};

use naia_client::{
    shared::{ChannelIndex, Protocolize},
    Client, Event,
};

use naia_bevy_shared::WorldProxyMut;

use crate::events::{
    DespawnEntityEvent, InsertComponentEvent, MessageEvent, RemoveComponentEvent, SpawnEntityEvent,
    UpdateComponentEvent,
};

use super::resource::ClientResource;

pub fn before_receive_events<P: Protocolize, C: ChannelIndex>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<P, Entity, C>>| {
        world.resource_scope(|world, mut client_resource: Mut<ClientResource>| {
            let event_results = client.receive(world.proxy_mut());

            unsafe {
                let mut spawn_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<SpawnEntityEvent>>()
                    .unwrap();
                let mut despawn_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<DespawnEntityEvent>>()
                    .unwrap();
                let mut insert_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<InsertComponentEvent<P::Kind>>>()
                    .unwrap();
                let mut update_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<UpdateComponentEvent<P::Kind>>>()
                    .unwrap();
                let mut remove_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<RemoveComponentEvent<P>>>()
                    .unwrap();
                let mut message_event_writer = world
                    .get_resource_unchecked_mut::<Events<MessageEvent<P, C>>>()
                    .unwrap();

                for event_result in event_results {
                    match event_result {
                        Ok(Event::Connection(_)) => {
                            client_resource.connector.set();
                            continue;
                        }
                        Ok(Event::Disconnection(_)) => {
                            client_resource.disconnector.set();
                            continue;
                        }
                        Ok(Event::Tick) => {
                            client_resource.ticker.set();
                            continue;
                        }
                        Ok(Event::SpawnEntity(entity)) => {
                            spawn_entity_event_writer.send(SpawnEntityEvent(entity));
                        }
                        Ok(Event::DespawnEntity(entity)) => {
                            despawn_entity_event_writer.send(DespawnEntityEvent(entity));
                        }
                        Ok(Event::InsertComponent(entity, component)) => {
                            insert_component_event_writer
                                .send(InsertComponentEvent(entity, component));
                        }
                        Ok(Event::RemoveComponent(entity, component)) => {
                            remove_component_event_writer
                                .send(RemoveComponentEvent(entity, component));
                        }
                        Ok(Event::Message(channel, message)) => {
                            message_event_writer.send(MessageEvent(channel, message));
                        }
                        Ok(Event::UpdateComponent(tick, entity, component)) => {
                            update_component_event_writer
                                .send(UpdateComponentEvent(tick, entity, component));
                        }
                        Err(_) => {}
                    }
                }
            }
        });
    });
}

pub fn should_connect(resource: Res<ClientResource>) -> ShouldRun {
    if resource.connector.is_set() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

pub fn finish_connect(mut resource: ResMut<ClientResource>) {
    resource.connector.reset();
}

pub fn should_disconnect(resource: Res<ClientResource>) -> ShouldRun {
    if resource.disconnector.is_set() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

pub fn finish_disconnect(mut resource: ResMut<ClientResource>) {
    resource.disconnector.reset();
}

pub fn should_tick(resource: Res<ClientResource>) -> ShouldRun {
    if resource.ticker.is_set() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

pub fn finish_tick(mut resource: ResMut<ClientResource>) {
    resource.ticker.reset();
}

pub fn should_receive<P: Protocolize, C: ChannelIndex>(
    client: Res<Client<P, Entity, C>>,
) -> ShouldRun {
    if client.is_connecting() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}
