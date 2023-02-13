use bevy_ecs::event::Events;
use bevy_ecs::{
    entity::Entity,
    schedule::ShouldRun,
    system::{Res, ResMut},
    world::{Mut, World},
};

use naia_client::Client;

use naia_bevy_shared::WorldProxyMut;

mod naia_events {
    pub use naia_client::{
        ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, InsertComponentEvent,
        MessageEvent, RejectEvent, RemoveComponentEvent, SpawnEntityEvent, TickEvent,
        UpdateComponentEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, InsertComponentEvent,
        MessageEvents, RejectEvent, RemoveComponentEvents, SpawnEntityEvent, UpdateComponentEvent,
    };
}

use super::{
    events::{
        DespawnEntityEvent, InsertComponentEvent, RemoveComponentEvents, SpawnEntityEvent,
        UpdateComponentEvent,
    },
    resource::ClientResource,
};

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<Entity>>| {
        world.resource_scope(|world, mut client_resource: Mut<ClientResource>| {
            let mut events = client.receive(world.proxy_mut());
            if !events.is_empty() {
                unsafe {
                    // Connect Event
                    let mut connect_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::ConnectEvent>>()
                        .unwrap();
                    for _ in events.read::<naia_events::ConnectEvent>() {
                        connect_event_writer.send(bevy_events::ConnectEvent);
                    }

                    // Disconnect Event
                    let mut disconnect_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::DisconnectEvent>>()
                        .unwrap();
                    for _ in events.read::<naia_events::DisconnectEvent>() {
                        disconnect_event_writer.send(bevy_events::DisconnectEvent);
                    }

                    // Tick Event
                    for _ in events.read::<naia_events::TickEvent>() {
                        client_resource.ticker.set();
                    }

                    // Error Event
                    let mut error_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::ErrorEvent>>()
                        .unwrap();
                    for error in events.read::<naia_events::ErrorEvent>() {
                        error_event_writer.send(bevy_events::ErrorEvent(error));
                    }

                    // Message Event
                    let mut message_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::MessageEvents>>()
                        .unwrap();
                    message_event_writer.send(bevy_events::MessageEvents::from(&mut events));

                    todo!();
                    let mut spawn_entity_event_writer = world
                        .get_resource_unchecked_mut::<Events<SpawnEntityEvent>>()
                        .unwrap();
                    let mut despawn_entity_event_writer = world
                        .get_resource_unchecked_mut::<Events<DespawnEntityEvent>>()
                        .unwrap();
                    let mut insert_component_event_writer = world
                        .get_resource_unchecked_mut::<Events<InsertComponentEvent>>()
                        .unwrap();
                    let mut update_component_event_writer = world
                        .get_resource_unchecked_mut::<Events<UpdateComponentEvent>>()
                        .unwrap();
                    let mut remove_component_event_writer = world
                        .get_resource_unchecked_mut::<Events<RemoveComponentEvents>>()
                        .unwrap();

                    // for event_result in event_results {
                    //     match event_result {
                    //         Ok(Event::SpawnEntity(entity)) => {
                    //             spawn_entity_event_writer.send(SpawnEntityEvent(entity));
                    //         }
                    //         Ok(Event::DespawnEntity(entity)) => {
                    //             despawn_entity_event_writer.send(DespawnEntityEvent(entity));
                    //         }
                    //         Ok(Event::InsertComponent(entity, component)) => {
                    //             insert_component_event_writer
                    //                 .send(InsertComponentEvent(entity, component));
                    //         }
                    //         Ok(Event::RemoveComponent(entity, component)) => {
                    //             remove_component_event_writer
                    //                 .send(RemoveComponentEvent(entity, component));
                    //         }
                    //         Ok(Event::Message(channel, message)) => {
                    //             message_event_writer.send(MessageEvent(channel, message));
                    //         }
                    //         Ok(Event::UpdateComponent(tick, entity, component)) => {
                    //             update_component_event_writer
                    //                 .send(UpdateComponentEvent(tick, entity, component));
                    //         }
                    //     }
                    // }
                }
            }
        });
    });
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

pub fn should_receive(client: Res<Client<Entity>>) -> ShouldRun {
    if client.is_connecting() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}
