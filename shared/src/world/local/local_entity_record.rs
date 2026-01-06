use crate::{HostEntity, OwnedLocalEntity, RemoteEntity};

#[derive(Debug)]
pub struct LocalEntityRecord {
    entity: OwnedLocalEntity,
    // delegated: bool,
}

impl LocalEntityRecord {
    pub fn new_host_owned_entity(entity: HostEntity) -> Self {
        Self {
            entity: OwnedLocalEntity::new_host(entity),
            // delegated: false,
        }
    }

    pub fn new_remote_owned_entity(entity: RemoteEntity) -> Self {
        Self {
            entity: OwnedLocalEntity::new_remote(entity),
            // delegated: false,
        }
    }

    pub fn is_host_owned(&self) -> bool {
        self.entity.is_host()
    }

    pub fn is_remote_owned(&self) -> bool {
        self.entity.is_remote()
    }

    // pub fn is_delegated(&self) -> bool {
    //     self.delegated
    // }

    pub(crate) fn host_entity(&self) -> HostEntity {
        self.entity.host()
    }

    pub(crate) fn remote_entity(&self) -> RemoteEntity {
        self.entity.remote()
    }

    pub(crate) fn owned_entity(&self) -> OwnedLocalEntity {
        self.entity
    }
}
