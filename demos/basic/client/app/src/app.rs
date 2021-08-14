use std::collections::HashMap;

use log::{info, warn};

use hecs::{Entity as HecsEntityKey, World, EntityBuilder as HecsEntityBuilder};

use naia_client::{ClientConfig, ClientEvent, Client, NaiaKey, LocalEntityKey as NaiaEntityKey, Ref, State};

use naia_demo_basic_shared::{
    get_shared_config, manifest_load,
    protocol::{Protocol, Auth, StringMessage},
};

pub struct App {
    client: Client<Protocol>,
    world: World,
    entity_builder: HecsEntityBuilder,
    entity_key_map: HashMap<NaiaEntityKey, HecsEntityKey>,
    server_event_count: u32,
}

impl App {
    pub fn new(client_config: ClientConfig) -> App {
        info!("Naia Client Example Started");

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = Protocol::Auth(Auth::new("charlie", "12345").state_wrap());

        App {
            client: Client::new(
                manifest_load(),
                Some(client_config),
                get_shared_config(),
                Some(auth),
            ),
            world: World::new(),
            entity_builder: HecsEntityBuilder::new(),
            entity_key_map: HashMap::new(),
            server_event_count: 0,
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
                        ClientEvent::Connection => {
                            info!("Client connected to: {}", self.client.server_address());
                        }
                        ClientEvent::Disconnection => {
                            info!("Client disconnected from: {}", self.client.server_address());
                        }
                        ClientEvent::Event(event_type) => match event_type {
                            Protocol::StringMessage(message_ref) => {
                                let _message = message_ref.borrow().message.get();
                                //info!("Client received event: {}", message);

                                let new_message =
                                    format!("Client Packet ({})", self.server_event_count);
                                //info!("Client send: {}", new_message);

                                let string_event = StringMessage::new(new_message);
                                self.client.send_event(&string_event);
                                self.server_event_count += 1;
                            }
                            _ => {}
                        },
                        ClientEvent::CreateEntity(naia_entity_key, component_keys) => {
                            info!("creation of entity: {}", naia_entity_key.to_u16());

                            // initialize w/ starting components
                            for component_key in component_keys {
                                info!("init component: {}, to entity: {}", component_key.to_u16(), naia_entity_key.to_u16());
                                let component = self.client.get_component(&component_key)
                                    .expect("attempting to add non-existent component to entity")
                                    .clone();
                                match component {
                                    Protocol::Position(position_ref) => {
                                        self.entity_builder.add(position_ref);
                                    }
                                    Protocol::Name(name_ref) => {
                                        self.entity_builder.add(name_ref);
                                    }
                                    Protocol::Marker(marker_ref) => {
                                        self.entity_builder.add(marker_ref);
                                    }
                                    _ => {}
                                }
                            }

                            let hecs_entity_key = self.world.spawn(self.entity_builder.build());
                            self.entity_key_map.insert(naia_entity_key, hecs_entity_key);
                        },
                        ClientEvent::DeleteEntity(naia_entity_key) => {
                            info!("deletion of entity: {}", naia_entity_key.to_u16());
                            if let Some(hecs_entity_key) = self.entity_key_map.remove(&naia_entity_key) {
                                self.world.despawn(hecs_entity_key)
                                    .expect("unsuccessful despawn of entity");
                            } else {
                                warn!("attempted deletion of non-existent entity");
                            }
                        },
                        ClientEvent::AddComponent(naia_entity_key, component_key) => {
                            info!("add component: {}, to entity: {}", component_key.to_u16(), naia_entity_key.to_u16());

                            let hecs_entity_key = *self.entity_key_map.get(&naia_entity_key)
                                .expect("attempting to add new component to non-existent entity");

                            let component = self.client.get_component(&component_key)
                                .expect("attempting to add non-existent component to entity")
                                .clone();
                            match component {
                                Protocol::Position(position_ref) => {
                                    self.world.insert_one(hecs_entity_key, position_ref)
                                        .expect("error inserting component");
                                }
                                Protocol::Name(name_ref) => {
                                    self.world.insert_one(hecs_entity_key, name_ref)
                                        .expect("error inserting component");
                                }
                                Protocol::Marker(marker_ref) => {
                                    self.world.insert_one(hecs_entity_key, marker_ref)
                                        .expect("error inserting component");
                                }
                                _ => {}
                            }
                        },
                        ClientEvent::RemoveComponent(naia_entity_key, component_key, component_ref) => {
                            info!("remove component: {}, from entity: {}", component_key.to_u16(), naia_entity_key.to_u16());
                            if self.entity_key_map.contains_key(&naia_entity_key) {
                                let hecs_entity_key = *self.entity_key_map.get(&naia_entity_key).unwrap();

                                match component_ref {
                                    Protocol::Position(position_ref) => {
                                        self.remove_component(&hecs_entity_key, &position_ref);
                                    }
                                    Protocol::Name(name_ref) => {
                                        self.remove_component(&hecs_entity_key, &name_ref);
                                    }
                                    Protocol::Marker(marker_ref) => {
                                        self.remove_component(&hecs_entity_key, &marker_ref);
                                    }
                                    _ => {}
                                }

                            } else {
                                warn!("attempting to remove component from non-existent entity");
                            }
                        },
                        ClientEvent::Tick => {
                            //info!("tick event");
                        },
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

    fn remove_component<T: 'static + State<Protocol>>(&mut self, hecs_entity_key: &HecsEntityKey, _component_ref: &Ref<T>) {
        self.world.remove_one::<Ref<T>>(*hecs_entity_key)
            .expect("error removing component");
    }
}
