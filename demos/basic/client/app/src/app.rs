cfg_if! {
    if #[cfg(feature = "mquad")] {
        use miniquad::info;
    } else {
        use log::info;
    }
}

use naia_client::{
    shared::{default_channels::UnorderedReliableChannel, SocketConfig},
    transport::webrtc,
    Client as NaiaClient, ClientConfig, ClientTickEvent, ConnectEvent, DespawnEntityEvent,
    DisconnectEvent, ErrorEvent, MessageEvent, RejectEvent, RemoveComponentEvent, SpawnEntityEvent,
    UpdateComponentEvent,
};

use naia_demo_world::{Entity, World};

use naia_basic_demo_shared::{protocol, Auth, Character, StringMessage};

type Client = NaiaClient<Entity>;

pub struct App {
    client: Client,
    world: World,
    message_count: u32,
    socket_config: SocketConfig,
}

impl App {
    pub fn default() -> Self {
        info!("Basic Naia Client Demo started");

        let protocol = protocol();
        let socket_config = protocol.socket.clone();
        let socket = webrtc::Socket::new("http://127.0.0.1:14191", &socket_config);
        let mut client = Client::new(ClientConfig::default(), protocol);

        // Incorrect auth here to force a rejection
        let auth = Auth::new("ronald", "12345");
        client.auth(auth);

        client.connect(socket);

        App {
            client,
            world: World::default(),
            message_count: 0,
            socket_config,
        }
    }

    pub fn update(&mut self) {
        if self.client.is_disconnected() {
            return;
        }

        let mut events = self.client.receive(self.world.proxy_mut());

        for server_address in events.read::<ConnectEvent>() {
            info!("Client connected to: {}", server_address);
        }
        for server_address in events.read::<RejectEvent>() {
            info!(
                "Client received unauthorized response from: {}",
                server_address
            );

            // Now give the correct username / password
            let auth = Auth::new("charlie", "12345");
            self.client.auth(auth);

            let socket = webrtc::Socket::new("http://127.0.0.1:14191", &self.socket_config);
            self.client.connect(socket);
        }
        for server_address in events.read::<DisconnectEvent>() {
            info!("Client disconnected from: {}", server_address);
        }
        for message in events.read::<MessageEvent<UnorderedReliableChannel, StringMessage>>() {
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
        for entity in events.read::<SpawnEntityEvent>() {
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
        for _ in events.read::<DespawnEntityEvent>() {
            info!("deletion of Character entity");
        }
        for (_, entity) in events.read::<UpdateComponentEvent<Character>>() {
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
        for (_, character) in events.read::<RemoveComponentEvent<Character>>() {
            info!(
                "data delete of Character - x: {}, y: {}, name: {} {}",
                *character.x,
                *character.y,
                (*character.fullname).first,
                (*character.fullname).last,
            );
        }
        for _ in events.read::<ClientTickEvent>() {
            //info!("tick event");
        }
        for error in events.read::<ErrorEvent>() {
            info!("Client Error: {}", error);
            return;
        }
    }
}
