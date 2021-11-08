use log::info;

use naia_hecs_client::{Event, WorldProxyMut};

use naia_hecs_demo_shared::protocol::{Protocol, StringMessage};

use crate::app::App;

pub fn process_events(app: &mut App) {
    for event in app.client.receive(app.world.proxy_mut(&mut app.world_data)) {
        match event {
            Ok(Event::Connection) => {
                info!("Client connected to: {}", app.client.server_address());
            }
            Ok(Event::Disconnection) => {
                info!("Client disconnected from: {}", app.client.server_address());
            }
            Ok(Event::Message(Protocol::StringMessage(_))) => {
                let send_message_contents =
                    format!("Client Packet (message {})", app.message_count);
                let send_message = StringMessage::new(send_message_contents);
                app.client.send_message(&send_message, true);
                app.message_count += 1;
            }
            Ok(Event::SpawnEntity(_, _)) => {
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
