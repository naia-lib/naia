use std::collections::HashMap;

use naia_server::UserKey;

use crate::harness::client_state::ClientState;
use super::keys::ClientKey;

/// Lightweight handle for ClientKey -> UserKey mapping
/// Allows ServerMut to map ClientKey to UserKey without holding full Scenario reference
pub struct Users<'a> {
    client_to_user: &'a HashMap<ClientKey, ClientState>,
    user_to_client: &'a HashMap<UserKey, ClientKey>,
}

impl<'a> Users<'a> {
    pub(crate) fn new(
        client_to_user: &'a HashMap<ClientKey, ClientState>,
        user_to_client: &'a HashMap<UserKey, ClientKey>,
    ) -> Self {
        Self {
            client_to_user,
            user_to_client,
        }
    }

    pub fn client_to_user_key(&self, client_key: &ClientKey) -> Option<UserKey> {
        self.client_to_user.get(&client_key)?.user_key()
    }

    /// Reverse lookup: find ClientKey for a given UserKey
    pub(crate) fn user_to_client_key(&self, user_key: &UserKey) -> Option<ClientKey> {
        self.user_to_client.get(user_key).cloned()
    }
}

