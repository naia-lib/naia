use naia_client::{Client as NaiaClient};
use naia_server::UserKey;

use crate::{TestWorld, TestEntity};

type Client = NaiaClient<TestEntity>;

pub(crate) struct ClientState {
    client: Client,
    world: TestWorld,
    user_key: UserKey,
}

impl ClientState {
    pub(crate) fn new(client: Client, world: TestWorld, user_key: UserKey) -> Self {
        Self { client, world, user_key }
    }

    /// Get reference to the client
    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    /// Get mutable reference to the client
    pub(crate) fn client_mut(&mut self) -> &mut Client {
        &mut self.client
    }
    
    /// Get reference to the world
    pub(crate) fn world(&self) -> &TestWorld {
        &self.world
    }

    /// Get mutable references to both client and world
    /// The compiler understands these are disjoint fields and allows this pattern
    pub(crate) fn client_and_world_mut(&mut self) -> (&mut Client, &mut TestWorld) {
        (&mut self.client, &mut self.world)
    }

    pub(crate) fn user_key(&self) -> UserKey {
        self.user_key
    }
}