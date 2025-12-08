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

    /// Get mutable reference to the client
    pub(crate) fn client_mut(&mut self) -> &mut Client {
        &mut self.client
    }

    /// Get reference to the client
    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    /// Get reference to the world
    pub(crate) fn world(&self) -> &TestWorld {
        &self.world
    }

    /// Get mutable references to both client and world
    /// This is a workaround for borrow checker limitations when both are needed
    pub(crate) fn client_and_world_mut(&mut self) -> (&mut Client, &mut TestWorld) {
        // Safe because Client and TestWorld are different fields
        unsafe {
            let client_ptr = &mut self.client as *mut Client;
            let world_ptr = &mut self.world as *mut TestWorld;
            (&mut *client_ptr, &mut *world_ptr)
        }
    }

    pub(crate) fn user_key(&self) -> UserKey {
        self.user_key
    }
}