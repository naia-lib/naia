//! Interior Visibility Feature
//!
//! This module provides testing-oriented APIs for accessing LocalEntity mappings.
//! It is only available when the `interior_visibility` feature is enabled.
//!
//! LocalEntity is a per-connection 16-bit identifier assigned to each replicated entity.
//! - On the server: The LocalEntity namespace is per-UserKey (entities are RemoteEntity from the user's perspective)
//! - On the client: The LocalEntity namespace is per-client instance (entities are RemoteEntity from the client's perspective)
//!
//! Note: LocalEntity IDs may be reused after an entity is fully removed and its replication state is cleaned up.
//! These APIs are meant for inspection/testing, not for long-term stable IDs across sessions.

use crate::{HostEntity, OwnedLocalEntity, RemoteEntity};

/// LocalEntity is a per-connection 16-bit identifier assigned to each replicated entity.
///
/// On the server, the LocalEntity namespace is per-UserKey.
/// On the client, the LocalEntity namespace is per-client instance.
///
/// Note: IDs may be reused after entity removal. These are for testing/introspection only.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct LocalEntity {
    inner: OwnedLocalEntity
}

impl LocalEntity {
    fn new(inner: OwnedLocalEntity) -> Self {
        Self {
            inner
        }
    }
}

impl From<HostEntity> for LocalEntity {
    fn from(entity: HostEntity) -> Self {
        Self::from(entity.copy_to_owned())
    }
}

impl From<RemoteEntity> for LocalEntity {
    fn from(entity: RemoteEntity) -> Self {
        Self::from(entity.copy_to_owned())
    }
}

impl From<OwnedLocalEntity> for LocalEntity {
    fn from(entity: OwnedLocalEntity) -> Self {
        Self::new(entity)
    }
}

impl Into<OwnedLocalEntity> for LocalEntity {
    fn into(self) -> OwnedLocalEntity {
        self.inner
    }
}