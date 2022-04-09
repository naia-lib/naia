use log::info;

use naia_hecs_client::Event;

use crate::app::App;

pub fn process_events(app: &mut App) {
    for event in app.client.receive(&mut app.world) {
        match event {
            Ok(Event::Connection(server_address)) => {
                info!("Client connected to: {}", server_address);
            }
            Ok(Event::Disconnection(server_address)) => {
                info!("Client disconnected from: {}", server_address);
            }
            Ok(Event::SpawnEntity(entity)) => {
                let new_id = app.next_id;
                app.next_id = app.next_id.wrapping_add(1);
                app.entity_to_id_map.insert(entity, new_id);
                info!("creation of entity: {new_id}");
            }
            Ok(Event::DespawnEntity(entity)) => {
                let id = app.entity_to_id_map.remove(&entity).unwrap();
                info!("deletion of entity: {id}");
            }
            Ok(Event::InsertComponent(entity, _)) => {
                let id = app.entity_to_id_map.get(&entity).unwrap();
                info!("insert component into entity: {id}");
            }
            Ok(Event::RemoveComponent(entity, _)) => {
                let id = app.entity_to_id_map.get(&entity).unwrap();
                info!("remove component from entity: {id}");
            }
            Ok(Event::Tick) => app.tick(),
            Err(err) => {
                info!("Naia Client Error: {}", err);
            }
            _ => {}
        }
    }
}
