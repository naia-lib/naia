cfg_if! {
    if #[cfg(feature = "mquad")] {
        use miniquad::info;
    } else {
        use log::info;
    }
}

use naia_client::{
    shared::{DefaultChannels, Protocolize},
    Client as NaiaClient, ClientConfig, Event,
};

use naia_demo_world::{Entity, World as DemoWorld};

use naia_basic_demo_shared::{
    protocol::{Auth, Character, Protocol},
    shared_config,
};

type World = DemoWorld<Protocol>;
type Client = NaiaClient<Protocol, Entity, DefaultChannels>;

pub struct App {
    client: Client,
    world: World,
    message_count: u32,
}

impl Default for App {
    fn default() -> Self {
        info!("Basic Naia Client Demo started");

        let auth = Auth::new("charlie", "12345");

        let mut client = Client::new(&ClientConfig::default(), &shared_config());
        client.auth(auth);
        client.connect("http://127.0.0.1:14191");

        App {
            client,
            world: World::default(),
            message_count: 0,
        }
    }
}

impl App {
    pub fn update(&mut self) {
        for event in self.client.receive(self.world.proxy_mut()) {
            match event {
                Ok(Event::Connection(server_address)) => {
                    info!("Client connected to: {}", server_address);
                }
                Ok(Event::Disconnection(server_address)) => {
                    info!("Client disconnected from: {}", server_address);
                }
                Ok(Event::Message(_, Protocol::StringMessage(message))) => {
                    let message_contents = &(*message.contents);
                    info!("Client recv <- {}", message_contents);

                    // let new_message_contents = format!("Client Message ({})",
                    // self.message_count); info!("Client send -> {}",
                    // new_message_contents);
                    //
                    // let string_message = StringMessage::new(new_message_contents);
                    // self.client.send_message(DefaultChannels::UnorderedUnreliable,
                    // &string_message);
                    self.message_count += 1;
                }
                Ok(Event::SpawnEntity(entity)) => {
                    if let Some(character) = self
                        .client
                        .entity(self.world.proxy(), &entity)
                        .component::<Character>()
                    {
                        info!(
                            "creation of Character - x: {}, y: {}, name: {} {}",
                            *character.x,
                            *character.y,
                            (*character.fullname).first,
                            (*character.fullname).last,
                        );
                    }
                }
                Ok(Event::UpdateComponent(_, entity, _)) => {
                    if let Some(character) = self
                        .client
                        .entity(self.world.proxy(), &entity)
                        .component::<Character>()
                    {
                        info!(
                            "update of Character - x: {}, y: {}, name: {} {}",
                            *character.x,
                            *character.y,
                            (*character.fullname).first,
                            (*character.fullname).last,
                        );
                    }
                }
                Ok(Event::RemoveComponent(_, component_protocol)) => {
                    if let Some(character) = component_protocol.cast_ref::<Character>() {
                        info!(
                            "data delete of Character - x: {}, y: {}, name: {} {}",
                            *character.x,
                            *character.y,
                            (*character.fullname).first,
                            (*character.fullname).last,
                        );
                    }
                }
                Ok(Event::DespawnEntity(_)) => {
                    info!("deletion of Character entity");
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
