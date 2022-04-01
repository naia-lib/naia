use log::info;

use naia_hecs_client::{Event, WorldProxyMut};

use crate::app::App;

pub fn process_events(app: &mut App) {
    for event in app.client.receive(app.world.proxy_mut(&mut app.world_data)) {
        match event {
            Ok(Event::Connection(server_address)) => {
                info!("Client connected to: {}", server_address);
            }
            Ok(Event::Disconnection(server_address)) => {
                info!("Client disconnected from: {}", server_address);
            }
            Ok(Event::SpawnEntity(_)) => {
                info!("creation of entity");
            }
            Ok(Event::DespawnEntity(_)) => {
                info!("deletion of entity");
            }
            Ok(Event::InsertComponent(_, _)) => {
                info!("insert component into entity");
            }
            Ok(Event::RemoveComponent(_, _)) => {
                info!("remove component from entity");
            }
            Ok(Event::Tick) => app.tick(),
            Err(err) => {
                info!("Naia Client Error: {}", err);
            }
            _ => {}
        }
    }
}
