use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
    collections::HashMap,
};

use log::{info, warn};

use hecs::{Entity as HecsEntityKey, World, EntityBuilder as HecsEntityBuilder};

use naia_client::{ClientConfig, ClientEvent, Client, NaiaKey, LocalEntityKey as NaiaEntityKey, Ref, Actor};

use naia_example_shared::{
    get_shared_config, manifest_load,
    components::Components,
    events::{Events, Auth, StringMessage},
};

const SERVER_PORT: u16 = 14191;

pub struct App {
    client: Client<Events, Components>,
    world: World,
    entity_builder: HecsEntityBuilder,
    entity_key_map: HashMap<NaiaEntityKey, HecsEntityKey>,
    server_event_count: u32,
}

impl App {
    pub fn new() -> App {
        info!("Naia Client Example Started");

        // Put your Server's IP Address here!, can't easily find this automatically from
        // the browser
        let server_ip_address: IpAddr = "127.0.0.1"
            .parse()
            .expect("couldn't parse input IP address");
        let server_socket_address = SocketAddr::new(server_ip_address, SERVER_PORT);

        let mut client_config = ClientConfig::default();
        client_config.heartbeat_interval = Duration::from_secs(2);
        // Keep in mind that the disconnect timeout duration should always be at least
        // 2x greater than the heartbeat interval, to make it so at the worst case, the
        // server would need to miss 2 heartbeat signals before disconnecting from a
        // given client
        client_config.disconnection_timeout_duration = Duration::from_secs(5);

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = Events::Auth(Auth::new("charlie", "12345"));

        App {
            client: Client::new(
                server_socket_address,
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
                            Events::StringMessage(string_event) => {
                                let _message = string_event.message.get();
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
                                    Components::Position(position_ref) => {
                                        self.entity_builder.add(position_ref);
                                    }
                                    Components::Name(name_ref) => {
                                        self.entity_builder.add(name_ref);
                                    }
                                    Components::Marker(marker_ref) => {
                                        self.entity_builder.add(marker_ref);
                                    }
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
                                Components::Position(position_ref) => {
                                    self.world.insert_one(hecs_entity_key, position_ref)
                                        .expect("error inserting component");
                                }
                                Components::Name(name_ref) => {
                                    self.world.insert_one(hecs_entity_key, name_ref)
                                        .expect("error inserting component");
                                }
                                Components::Marker(marker_ref) => {
                                    self.world.insert_one(hecs_entity_key, marker_ref)
                                        .expect("error inserting component");
                                }
                            }
                        },
                        ClientEvent::RemoveComponent(naia_entity_key, component_key, component_ref) => {
                            info!("remove component: {}, from entity: {}", component_key.to_u16(), naia_entity_key.to_u16());
                            if self.entity_key_map.contains_key(&naia_entity_key) {
                                let hecs_entity_key = *self.entity_key_map.get(&naia_entity_key).unwrap();

                                match component_ref {
                                    Components::Position(position_ref) => {
                                        self.remove_component(&hecs_entity_key, &position_ref);
                                    }
                                    Components::Name(name_ref) => {
                                        self.remove_component(&hecs_entity_key, &name_ref);
                                    }
                                    Components::Marker(marker_ref) => {
                                        self.remove_component(&hecs_entity_key, &marker_ref);
                                    }
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

    fn remove_component<T: 'static + Actor<Components>>(&mut self, hecs_entity_key: &HecsEntityKey, _component_ref: &Ref<T>) {
        self.world.remove_one::<Ref<T>>(*hecs_entity_key)
            .expect("error removing component");
    }
}
