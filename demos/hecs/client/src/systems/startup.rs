use std::collections::HashMap;

use hecs::World;

use naia_hecs_client::{ClientConfig, Protocol};

use naia_hecs_demo_shared::Auth;

use crate::app::{App, Client};

pub fn app_init(
    client_config: ClientConfig,
    mut protocol: Protocol,
    server_addr: &str,
    auth: Auth,
) -> App {
    let world = protocol.wrap_world(World::new());
    let mut client = Client::new(client_config, protocol);
    client.auth(auth);
    client.connect(server_addr);

    App {
        client,
        world,
        message_count: 0,
        entity_to_id_map: HashMap::new(),
        next_id: 0,
    }
}
