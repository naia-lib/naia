use std::sync::Arc;

use parking_lot::Mutex;

use naia_client::Client as NaiaClient;
use naia_server::UserKey;
use naia_shared::IdentityToken;

use crate::{TestEntity, TestWorld};

type Client = NaiaClient<TestEntity>;

pub(crate) struct ClientState {
    client: Client,
    world: TestWorld,
    user_key_opt: Option<UserKey>,
    /// Shared handle to the identity token received from the server
    identity_token: Arc<Mutex<Option<IdentityToken>>>,
    /// Shared handle to the rejection code (if any) returned by the handshake
    rejection_code: Arc<Mutex<Option<u16>>>,
}

impl ClientState {
    pub(crate) fn new(
        client: Client,
        world: TestWorld,
        identity_token: Arc<Mutex<Option<IdentityToken>>>,
        rejection_code: Arc<Mutex<Option<u16>>>,
    ) -> Self {
        Self {
            client,
            world,
            user_key_opt: None,
            identity_token,
            rejection_code,
        }
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

    pub(crate) fn set_user_key(&mut self, user_key: UserKey) {
        self.user_key_opt = Some(user_key);
    }

    pub(crate) fn user_key(&self) -> Option<UserKey> {
        self.user_key_opt
    }

    /// Get the last identity token provided by the server to this client
    /// Returns None if no token has been received yet
    pub(crate) fn identity_token(&self) -> Option<IdentityToken> {
        self.identity_token.lock().clone()
    }

    /// Get the last rejection code (if any) returned by the handshake
    /// Returns None if no rejection occurred
    pub(crate) fn rejection_code(&self) -> Option<u16> {
        *self.rejection_code.lock()
    }

    /// Get a reference to the identity token handle for mutation
    /// This is used internally by ClientMutateCtx to allow setting tokens
    pub(crate) fn identity_token_handle(&self) -> &Arc<Mutex<Option<IdentityToken>>> {
        &self.identity_token
    }
}
