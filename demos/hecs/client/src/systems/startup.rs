use std::collections::HashMap;

use naia_hecs_client::{
    shared::{DefaultChannels, SharedConfig},
    ClientConfig, WorldWrapper as World,
};

use naia_hecs_demo_shared::protocol::Auth;

use crate::app::{App, Client};

pub fn app_init(
    client_config: ClientConfig,
    shared_config: SharedConfig<DefaultChannels>,
    server_addr: &str,
    auth: Auth,
) -> App {
    let mut client = Client::new(&client_config, &shared_config);
    client.auth(auth);
    client.connect(server_addr);

    App {
        client,
        world: World::new(),
        message_count: 0,
        entity_to_id_map: HashMap::new(),
        next_id: 0,
    }
}
