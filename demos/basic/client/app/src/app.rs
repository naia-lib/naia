use log::info;

use naia_client::{Client, ClientConfig, Event, ProtocolType};

use naia_basic_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Auth, Character, Protocol, StringMessage},
};

pub struct App {
    client: Client<Protocol>,
    message_count: u32,
}

impl App {
    pub fn new() -> Self {
        info!("Basic Naia Client Demo started");

        let mut client_config = ClientConfig::default();
        client_config.socket_config.server_address = get_server_address();

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = Auth::new("charlie", "12345");

        let client = Client::new(
            Protocol::load(),
            Some(client_config),
            get_shared_config(),
            Some(auth),
        );

        App {
            client,
            message_count: 0,
        }
    }

    // Currently, this will call every frame.
    // On Linux it's called in a loop.
    // On Web it's called via request_animation_frame()
    pub fn update(&mut self) {
        for event in self.client.receive() {
            match event {
                Ok(Event::Connection) => {
                    info!("Client connected to: {}", self.client.server_address());
                }
                Ok(Event::Disconnection) => {
                    info!("Client disconnected from: {}", self.client.server_address());
                }
                Ok(Event::Message(Protocol::StringMessage(message_ref))) => {
                    let message = message_ref.borrow();
                    let message_contents = message.contents.get();
                    info!("Client recv <- {}", message_contents);

                    let new_message_contents = format!("Client Message ({})", self.message_count);
                    info!("Client send -> {}", new_message_contents);

                    let string_message = StringMessage::new(new_message_contents);
                    self.client.queue_message(&string_message, true);
                    self.message_count += 1;
                }
                Ok(Event::SpawnEntity(entity_key, component_list)) => {
                    for component_protocol in component_list {
                        if let Some(character_ref) = component_protocol.to_typed_ref::<Character>()
                        {
                            let character = character_ref.borrow();
                            info!(
                                "creation of Character with key: {}, x: {}, y: {}, name: {} {}",
                                entity_key,
                                character.x.get(),
                                character.y.get(),
                                character.fullname.get().first,
                                character.fullname.get().last,
                            );
                        }
                    }
                }
                Ok(Event::UpdateComponent(entity_key, component_protocol)) => {
                    if let Some(character_ref) = component_protocol.to_typed_ref::<Character>() {
                        let character = character_ref.borrow();
                        info!(
                            "update of Character with key: {}, x: {}, y: {}, name: {} {}",
                            entity_key,
                            character.x.get(),
                            character.y.get(),
                            character.fullname.get().first,
                            character.fullname.get().last,
                        );
                    }
                }
                Ok(Event::RemoveComponent(entity_key, component_protocol)) => {
                    if let Some(character_ref) = component_protocol.to_typed_ref::<Character>() {
                        let character = character_ref.borrow();
                        info!(
                            "data delete of Character with key: {}, x: {}, y: {}, name: {} {}",
                            entity_key,
                            character.x.get(),
                            character.y.get(),
                            character.fullname.get().first,
                            character.fullname.get().last,
                        );
                    }
                }
                Ok(Event::DespawnEntity(entity_key)) => {
                    info!("deletion of Character Entity: {}", entity_key,);
                }
                Ok(Event::Tick) => {
                    //info!("tick event");
                }

                Err(err) => {
                    info!("Client Error: {}", err);
                    return;
                }
                _ => {}
            }
        }
    }
}
