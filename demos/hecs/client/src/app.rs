use std::collections::HashMap;

use log::{info, warn};

use hecs::{Entity as HecsEntityKey, EntityBuilder as HecsEntityBuilder, World};

use naia_client::{
    Client, ClientConfig, Event, LocalEntityKey as NaiaEntityKey, NaiaKey, Ref, Replicate,
};

use naia_hecs_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Auth, Marker, Name, Position, Protocol, StringMessage},
};

pub struct App {
    client: Client<Protocol>,
    world: World,
    entity_builder: HecsEntityBuilder,
    entity_key_map: HashMap<NaiaEntityKey, HecsEntityKey>,
    message_count: u32,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Hecs Client Demo started");

        let mut client_config = ClientConfig::default();
        client_config.socket_config.server_address = get_server_address();

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = Auth::new("charlie", "12345");

        let client = Client::new(Some(client_config), get_shared_config(), Some(auth));

        App {
            client,
            world: World::new(),
            entity_builder: HecsEntityBuilder::new(),
            entity_key_map: HashMap::new(),
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
                Ok(Event::Message(Protocol::StringMessage(_recv_message_ref))) => {
                    //let recv_message = _recv_message_ref.borrow();
                    //let recv_message_contents = recv_message.contents.get();
                    //info!("Client received message: {}", recv_message_contents);

                    let send_message_contents = format!("Client Packet ({})", self.message_count);
                    //info!("Client send: {}", send_message_contents);

                    let send_message = StringMessage::new(send_message_contents);
                    self.client.queue_message(&send_message, true);
                    self.message_count += 1;
                }
                Ok(Event::SpawnEntity(naia_entity_key, component_list)) => {
                    info!("creation of entity: {}", naia_entity_key.to_u16());

                    // initialize w/ starting components
                    for component_protocol in component_list {
                        match component_protocol {
                            Protocol::Position(position_ref) => {
                                self.entity_builder.add(position_ref);

                                info!(
                                    "init position component, to entity: {}",
                                    naia_entity_key.to_u16()
                                );
                            }
                            Protocol::Name(name_ref) => {
                                self.entity_builder.add(name_ref);

                                info!(
                                    "init name component, to entity: {}",
                                    naia_entity_key.to_u16()
                                );
                            }
                            Protocol::Marker(marker_ref) => {
                                self.entity_builder.add(marker_ref);

                                info!(
                                    "init marker component, to entity: {}",
                                    naia_entity_key.to_u16()
                                );
                            }
                            _ => {}
                        }
                    }

                    let hecs_entity_key = self.world.spawn(self.entity_builder.build());
                    self.entity_key_map.insert(naia_entity_key, hecs_entity_key);
                }
                Ok(Event::DespawnEntity(naia_entity_key)) => {
                    info!("deletion of entity: {}", naia_entity_key.to_u16());
                    if let Some(hecs_entity_key) = self.entity_key_map.remove(&naia_entity_key) {
                        self.world
                            .despawn(hecs_entity_key)
                            .expect("unsuccessful despawn of entity");
                    } else {
                        warn!("attempted deletion of non-existent entity");
                    }
                }
                Ok(Event::InsertComponent(naia_entity_key, _)) => {
                    let hecs_entity_key = *self
                        .entity_key_map
                        .get(&naia_entity_key)
                        .expect("attempting to add new component to non-existent entity");

                    if let Some(position_ref) =
                        self.client.entity(&naia_entity_key).component::<Position>()
                    {
                        self.world
                            .insert_one(hecs_entity_key, position_ref.clone())
                            .expect("error inserting component");

                        info!(
                            "add position component, to entity: {}",
                            naia_entity_key.to_u16()
                        );
                    }
                    if let Some(name_ref) = self.client.entity(&naia_entity_key).component::<Name>()
                    {
                        self.world
                            .insert_one(hecs_entity_key, name_ref.clone())
                            .expect("error inserting component");

                        info!(
                            "add name component, to entity: {}",
                            naia_entity_key.to_u16()
                        );
                    }
                    if let Some(marker_ref) =
                        self.client.entity(&naia_entity_key).component::<Marker>()
                    {
                        self.world
                            .insert_one(hecs_entity_key, marker_ref.clone())
                            .expect("error inserting component");

                        info!(
                            "add marker component, to entity: {}",
                            naia_entity_key.to_u16()
                        );
                    }
                }
                Ok(Event::RemoveComponent(naia_entity_key, _)) => {
                    if let Some(hecs_entity_key) =
                        self.entity_key_map.get(&naia_entity_key).copied()
                    {
                        if self
                            .client
                            .entity(&naia_entity_key) // this may not exist anymore because it was deleted...
                            .has_component::<Position>()
                        {
                            self.remove_component::<Position>(&hecs_entity_key);
                        }
                        if self.client.entity(&naia_entity_key).has_component::<Name>() {
                            self.remove_component::<Name>(&hecs_entity_key);
                        }
                        if self
                            .client
                            .entity(&naia_entity_key)
                            .has_component::<Marker>()
                        {
                            self.remove_component::<Marker>(&hecs_entity_key);
                        }
                    }
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

    fn remove_component<R: 'static + Replicate<Protocol>>(
        &mut self,
        hecs_entity_key: &HecsEntityKey,
    ) {
        self.world
            .remove_one::<Ref<R>>(*hecs_entity_key)
            .expect("error removing component");
    }
}
