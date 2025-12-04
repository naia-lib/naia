use std::collections::HashMap;

use naia_server::UserKey;

use super::keys::ClientKey;

/// Lightweight handle for ClientKey -> UserKey mapping
/// Allows ServerMut to map ClientKey to UserKey without holding full Scenario reference
pub struct Users<'a> {
    pub(crate) mapping: &'a HashMap<ClientKey, UserKey>,
}

impl<'a> Users<'a> {
    pub fn user_for_client(&self, client_key: ClientKey) -> Option<UserKey> {
        self.mapping.get(&client_key).copied()
    }
}

