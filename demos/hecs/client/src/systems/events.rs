use log::info;

use naia_hecs_client::{ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, InsertComponentEvent, RemoveComponentEvent, SpawnEntityEvent, TickEvent};
use naia_hecs_demo_shared::{Marker, Name, Position};

use crate::app::App;

pub fn process_events(app: &mut App) {
    if app.client.is_disconnected() {
        return;
    }

    let mut events = app.client.receive(&mut app.world);

    for server_address in events.read::<ConnectEvent>() {
        info!("Client connected to: {}", server_address);
    }
    for server_address in events.read::<DisconnectEvent>() {
        info!("Client disconnected from: {}", server_address);
    }
    for entity in events.read::<SpawnEntityEvent>() {
        let new_id = app.next_id;
        app.next_id = app.next_id.wrapping_add(1);
        app.entity_to_id_map.insert(entity, new_id);
        info!("creation of entity: {new_id}");
    }
    for entity in events.read::<DespawnEntityEvent>() {
        let id = app.entity_to_id_map.remove(&entity).unwrap();
        info!("deletion of entity: {id}");
    }
    for (entity, _) in events.read::<InsertComponentEvent>() {
        let id = app.entity_to_id_map.get(&entity).unwrap();
        info!("insert component into entity: {id}");
    }
    for (entity, _) in events.read::<RemoveComponentEvent<Marker>>() {
        let id = app.entity_to_id_map.get(&entity).unwrap();
        info!("remove Marker component from entity: {id}");
    }
    for (entity, _) in events.read::<RemoveComponentEvent<Name>>() {
        let id = app.entity_to_id_map.get(&entity).unwrap();
        info!("remove Name component from entity: {id}");
    }
    for (entity, _) in events.read::<RemoveComponentEvent<Position>>() {
        let id = app.entity_to_id_map.get(&entity).unwrap();
        info!("remove Position component from entity: {id}");
    }
    for _ in events.read::<TickEvent>() {
        app.tick();
    }
    for error in events.read::<ErrorEvent>() {
        info!("Naia Client Error: {}", error);
    }
}
