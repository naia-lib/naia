use std::time::Duration;

use log::info;

use naia_client::{
    Client, ClientConfig, Event, Replicate,
};

use naia_basic_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Auth, Protocol, StringMessage},
};

pub struct App {
    client: Client<Protocol>,
    message_count: u32,
}

impl App {
    pub fn new() -> App {
        info!("Basic Naia Client Demo started");

        let mut client_config = ClientConfig::default();

        // Put your Server's IP Address here!, can't easily find this automatically from
        // the browser
        client_config.server_address = get_server_address();

        client_config.heartbeat_interval = Duration::from_secs(2);
        // Keep in mind that the disconnect timeout duration should always be at least
        // 2x greater than the heartbeat interval, to make it so at the worst case, the
        // server would need to miss 2 heartbeat signals before disconnecting from a
        // given client
        client_config.disconnection_timeout_duration = Duration::from_secs(5);

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = Auth::new("charlie", "12345").to_protocol();

        App {
            client: Client::new(
                Protocol::load(),
                Some(client_config),
                get_shared_config(),
                Some(auth),
            ),
            message_count: 0,
        }
    }

    // Currently, this will call every frame.
    // On Linux it's called in a loop.
    // On Web it's called via request_animation_frame()
    pub fn update(&mut self) {
        loop {
            if let Some(result) = self.client.receive() {
                match result {
                    Ok(event) => match event {
                        Event::Connection => {
                            info!("Client connected to: {}", self.client.server_address());
                        }
                        Event::Disconnection => {
                            info!("Client disconnected from: {}", self.client.server_address());
                        }
                        Event::Message(protocol) => match protocol {
                            Protocol::StringMessage(message_ref) => {
                                let message = message_ref.borrow();
                                let message_contents = message.contents.get();
                                info!("Client recv <- {}", message_contents);

                                let new_message_contents = format!("Client Message ({})", self.message_count);
                                info!("Client send -> {}", new_message_contents);

                                let string_message = StringMessage::new(new_message_contents);
                                self.client.send_message(&string_message, true);
                                self.message_count += 1;
                            }
                            _ => {}
                        },
                        Event::CreateObject(object_key) => {
                            if let Some(Protocol::Character(character_ref)) = self.client.get_object(&object_key) {
                                let character = character_ref.borrow();
                                info!("creation of Character with key: {}, x: {}, y: {}, name: {} {}",
                                      object_key,
                                      character.x.get(),
                                      character.y.get(),
                                      character.fullname.get().first,
                                      character.fullname.get().last,
                                );
                            }
                        }
                        Event::UpdateObject(object_key) => {
                            if let Some(Protocol::Character(character_ref)) = self.client.get_object(&object_key) {
                                    let character = character_ref.borrow();
                                    info!("update of Character with key: {}, x: {}, y: {}, name: {} {}",
                                          object_key,
                                          character.x.get(),
                                          character.y.get(),
                                          character.fullname.get().first,
                                          character.fullname.get().last,
                                    );
                            }
                        }
                        Event::DeleteObject(object_key, protocol) => {
                            if let Protocol::Character(character_ref) = protocol {
                                let character = character_ref.borrow();
                                info!("deletion of Character with key: {}, x: {}, y: {}, name: {} {}",
                                      object_key,
                                      character.x.get(),
                                      character.y.get(),
                                      character.fullname.get().first,
                                      character.fullname.get().last,
                                );
                            }
                        }
                        Event::Tick => {
                            //info!("tick event");
                        }
                        _ => {}
                    },
                    Err(err) => {
                        info!("Client Error: {}", err);
                        return;
                    }
                }
            } else {
                break;
            }
        }
    }
}
