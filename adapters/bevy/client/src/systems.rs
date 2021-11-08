use bevy::{
    app::Events,
    ecs::{
        entity::Entity,
        schedule::ShouldRun,
        system::{Res, ResMut},
        world::{Mut, World},
    },
};

use naia_client::{Client, Event, ProtocolType};

use naia_bevy_shared::WorldProxyMut;

use super::{
    components::{Confirmed, Predicted},
    resource::ClientResource,
};
use crate::events::{
    DespawnEntityEvent, DisownEntityEvent, InsertComponentEvent, MessageEvent, NewCommandEvent,
    OwnEntityEvent, RemoveComponentEvent, ReplayCommandEvent, RewindEntityEvent, SpawnEntityEvent,
    UpdateComponentEvent,
};

pub fn before_receive_events<P: ProtocolType>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<P, Entity>>| {
        world.resource_scope(|world, mut client_resource: Mut<ClientResource>| {
            let event_results = client.receive(world.proxy_mut());

            let mut entities_to_spawn: Vec<Entity> = Vec::new();
            let mut entities_to_own: Vec<Entity> = Vec::new();

            unsafe {
                let mut spawn_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<SpawnEntityEvent<P>>>()
                    .unwrap();
                let mut despawn_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<DespawnEntityEvent>>()
                    .unwrap();
                let mut own_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<OwnEntityEvent>>()
                    .unwrap();
                let mut disown_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<DisownEntityEvent>>()
                    .unwrap();
                let mut rewind_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<RewindEntityEvent>>()
                    .unwrap();
                let mut insert_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<InsertComponentEvent<P>>>()
                    .unwrap();
                let mut update_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<UpdateComponentEvent<P>>>()
                    .unwrap();
                let mut remove_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<RemoveComponentEvent<P>>>()
                    .unwrap();
                let mut message_event_writer = world
                    .get_resource_unchecked_mut::<Events<MessageEvent<P>>>()
                    .unwrap();
                let mut new_command_event_writer = world
                    .get_resource_unchecked_mut::<Events<NewCommandEvent<P>>>()
                    .unwrap();
                let mut replay_command_event_writer = world
                    .get_resource_unchecked_mut::<Events<ReplayCommandEvent<P>>>()
                    .unwrap();

                for event_result in event_results {
                    match event_result {
                        Ok(Event::Connection) => {
                            client_resource.connector.set();
                            continue;
                        }
                        Ok(Event::Disconnection) => {
                            client_resource.disconnector.set();
                            continue;
                        }
                        Ok(Event::Tick) => {
                            client_resource.ticker.set();
                            continue;
                        }
                        Ok(Event::SpawnEntity(entity, components)) => {
                            entities_to_spawn.push(entity);
                            spawn_entity_event_writer
                                .send(SpawnEntityEvent::<P>(entity, components));
                        }
                        Ok(Event::OwnEntity(ref owned_entity)) => {
                            let predicted_entity = owned_entity.predicted;
                            entities_to_own.push(predicted_entity);
                            own_entity_event_writer.send(OwnEntityEvent(owned_entity.clone()));
                        }
                        Ok(Event::DespawnEntity(entity)) => {
                            despawn_entity_event_writer.send(DespawnEntityEvent(entity));
                        }
                        Ok(Event::DisownEntity(entity)) => {
                            disown_entity_event_writer.send(DisownEntityEvent(entity));
                        }
                        Ok(Event::RewindEntity(entity)) => {
                            rewind_entity_event_writer.send(RewindEntityEvent(entity));
                        }
                        Ok(Event::InsertComponent(entity, component)) => {
                            insert_component_event_writer
                                .send(InsertComponentEvent(entity, component));
                        }
                        Ok(Event::RemoveComponent(entity, component)) => {
                            remove_component_event_writer
                                .send(RemoveComponentEvent(entity, component));
                        }
                        Ok(Event::UpdateComponent(entity, component)) => {
                            update_component_event_writer
                                .send(UpdateComponentEvent(entity, component));
                        }
                        Ok(Event::Message(message)) => {
                            message_event_writer.send(MessageEvent(message));
                        }
                        Ok(Event::NewCommand(entity, command)) => {
                            new_command_event_writer.send(NewCommandEvent(entity, command));
                        }
                        Ok(Event::ReplayCommand(entity, command)) => {
                            replay_command_event_writer.send(ReplayCommandEvent(entity, command));
                        }
                        Err(_) => {}
                    }
                }
            }

            for entity in entities_to_spawn {
                world.entity_mut(entity).insert(Confirmed);
            }

            for entity in entities_to_own {
                world.entity_mut(entity).insert(Predicted);
            }
        });
    });
}

pub fn should_connect(resource: Res<ClientResource>) -> ShouldRun {
    if resource.connector.is_set() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn finish_connect(mut resource: ResMut<ClientResource>) {
    resource.connector.reset();
}

pub fn should_disconnect(resource: Res<ClientResource>) -> ShouldRun {
    if resource.disconnector.is_set() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn finish_disconnect(mut resource: ResMut<ClientResource>) {
    resource.disconnector.reset();
}

pub fn should_tick(resource: Res<ClientResource>) -> ShouldRun {
    if resource.ticker.is_set() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn finish_tick(mut resource: ResMut<ClientResource>) {
    resource.ticker.reset();
}
