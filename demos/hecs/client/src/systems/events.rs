use log::info;

use naia_hecs_client::{
    ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
    InsertComponentEvent, RemoveComponentEvent, SpawnEntityEvent,
};
use naia_hecs_demo_shared::{Marker, Name, Position};

use crate::app::App;

pub fn process_events(app: &mut App) {
    if !app.client.connection_status().is_connected() {
        return;
    }

    let mut events = app.client.receive(&mut app.world);

    // Connect Events
    for server_address in events.read::<ConnectEvent>() {
        info!("Client connected to: {}", server_address);
    }

    // Disconnect Events
    for server_address in events.read::<DisconnectEvent>() {
        info!("Client disconnected from: {}", server_address);
    }

    // Spawn Entity Events
    for entity in events.read::<SpawnEntityEvent>() {
        let new_id = app.next_id;
        app.next_id = app.next_id.wrapping_add(1);
        app.entity_to_id_map.insert(entity, new_id);
        info!("creation of entity: {new_id}");
    }

    // Insert Component Events
    for entity in events.read::<InsertComponentEvent<Marker>>() {
        let id = app.entity_to_id_map.get(&entity).unwrap();
        info!("insert Marker component into entity: {id}");
    }
    for entity in events.read::<InsertComponentEvent<Name>>() {
        let id = app.entity_to_id_map.get(&entity).unwrap();
        info!("insert Name component into entity: {id}");
    }
    for entity in events.read::<InsertComponentEvent<Position>>() {
        let id = app.entity_to_id_map.get(&entity).unwrap();
        info!("insert Position component into entity: {id}");
    }

    // Remove Component Events
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

    // Despawn Events
    for entity in events.read::<DespawnEntityEvent>() {
        let id = app.entity_to_id_map.remove(&entity).unwrap();
        info!("deletion of entity: {id}");
    }

    // Tick Events
    for _ in events.read::<ClientTickEvent>() {
        app.tick();
    }

    // Error Events
    for error in events.read::<ErrorEvent>() {
        info!("Naia Client Error: {}", error);
    }
}
