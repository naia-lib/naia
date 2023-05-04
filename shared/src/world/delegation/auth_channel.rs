use std::sync::{Arc, RwLock};

use crate::world::delegation::entity_auth_status::EntityAuthStatus;

// EntityAuthChannel
#[derive(Clone)]
pub(crate) struct EntityAuthChannel {
    data: Arc<RwLock<EntityAuthData>>,
}

impl EntityAuthChannel {
    pub(crate) fn new_channel() -> (EntityAuthMutator, EntityAuthAccessor) {
        let channel = Self {
            data: Arc::new(RwLock::new(EntityAuthData::new())),
        };

        let sender = EntityAuthMutator::new(&channel);
        let receiver = EntityAuthAccessor::new(&channel);

        (sender, receiver)
    }

    fn auth_status(&self) -> EntityAuthStatus {
        let data = self
            .data
            .as_ref()
            .read()
            .expect("Lock on AuthStatus is held by current thread.");
        return data.auth_status();
    }

    fn set_auth_status(&self, auth_status: EntityAuthStatus) {
        let mut data = self
            .data
            .as_ref()
            .write()
            .expect("Lock on AuthStatus is held by current thread.");
        data.set_auth_status(auth_status);
    }
}

// EntityAuthData
struct EntityAuthData {
    status: EntityAuthStatus,
}

impl EntityAuthData {
    fn new() -> Self {
        Self {
            status: EntityAuthStatus::Available,
        }
    }

    fn auth_status(&self) -> EntityAuthStatus {
        self.status
    }

    fn set_auth_status(&mut self, auth_status: EntityAuthStatus) {
        self.status = auth_status;
    }
}

// EntityAuthAccessor
#[derive(Clone)]
pub struct EntityAuthAccessor {
    channel: EntityAuthChannel,
}

impl EntityAuthAccessor {
    fn new(channel: &EntityAuthChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }

    pub(crate) fn auth_status(&self) -> EntityAuthStatus {
        self.channel.auth_status()
    }
}

// EntityAuthMutator
// no Clone necessary
pub(crate) struct EntityAuthMutator {
    channel: EntityAuthChannel,
}

impl EntityAuthMutator {
    fn new(channel: &EntityAuthChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }

    pub(crate) fn set_auth_status(&self, auth_status: EntityAuthStatus) {
        self.channel.set_auth_status(auth_status);
    }
}
