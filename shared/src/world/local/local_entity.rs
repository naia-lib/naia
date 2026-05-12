use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedVariableInteger};

use crate::{EntityDoesNotExistError, GlobalEntity, LocalEntityAndGlobalEntityConverter};

/// A connection-local entity ID that records whether the entity is host-owned or remote-owned.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum OwnedLocalEntity {
    /// Entity whose authoritative state originates on this side of the connection.
    Host {
        /// Wire-level entity ID within the host pool.
        id: u16,
        /// `true` if this entity belongs to the static pool.
        is_static: bool,
    },
    /// Entity whose authoritative state originates on the far side of the connection.
    Remote {
        /// Wire-level entity ID within the remote pool.
        id: u16,
        /// `true` if this entity belongs to the static pool.
        is_static: bool,
    },
}

impl OwnedLocalEntity {
    /// Creates a dynamic `Host` variant from a [`HostEntity`].
    pub fn new_host(id: HostEntity) -> Self {
        Self::Host { id: id.value(), is_static: false }
    }

    /// Creates a dynamic `Host` variant from a raw `u16` ID.
    pub fn new_host_dynamic(id: u16) -> Self {
        Self::Host { id, is_static: false }
    }

    /// Creates a static `Host` variant from a raw `u16` ID.
    pub fn new_host_static(id: u16) -> Self {
        Self::Host { id, is_static: true }
    }

    /// Creates a `Remote` variant from a [`RemoteEntity`], preserving its `is_static` flag.
    pub fn new_remote(id: RemoteEntity) -> Self {
        Self::Remote { id: id.value(), is_static: id.is_static() }
    }

    /// Creates a dynamic `Remote` variant from a raw `u16` ID.
    pub fn new_remote_dynamic(id: u16) -> Self {
        Self::Remote { id, is_static: false }
    }

    /// Creates a static `Remote` variant from a raw `u16` ID.
    pub fn new_remote_static(id: u16) -> Self {
        Self::Remote { id, is_static: true }
    }

    /// Returns `true` if this is a `Host` variant.
    pub fn is_host(&self) -> bool {
        match self {
            Self::Host { .. } => true,
            Self::Remote { .. } => false,
        }
    }

    /// Returns `true` if this is a `Remote` variant.
    pub fn is_remote(&self) -> bool {
        !self.is_host()
    }

    /// Returns `true` if this entity belongs to the static pool.
    pub fn is_static(&self) -> bool {
        match self {
            Self::Host { is_static, .. } => *is_static,
            Self::Remote { is_static, .. } => *is_static,
        }
    }

    /// Returns the raw `u16` wire ID for this entity, regardless of variant.
    pub fn id(&self) -> u16 {
        match self {
            Self::Host { id, .. } | Self::Remote { id, .. } => *id,
        }
    }

    /// Serializes this entity into the bit stream, writing host/remote flag, static flag, and ID.
    pub fn ser(&self, writer: &mut dyn BitWrite) {
        match self {
            Self::Host { id, is_static } => {
                true.ser(writer);
                is_static.ser(writer);
                UnsignedVariableInteger::<7>::new(*id).ser(writer);
            }
            Self::Remote { id, is_static } => {
                false.ser(writer);
                is_static.ser(writer);
                UnsignedVariableInteger::<7>::new(*id).ser(writer);
            }
        }
    }

    /// Deserializes an `OwnedLocalEntity` from the bit stream.
    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let is_host = bool::de(reader)?;
        let is_static = bool::de(reader)?;
        let id = UnsignedVariableInteger::<7>::de(reader)?.get() as u16;
        if is_host {
            Ok(Self::Host { id, is_static })
        } else {
            Ok(Self::Remote { id, is_static })
        }
    }

    /// Returns the encoded bit length of this entity.
    pub fn bit_length(&self) -> u32 {
        match self {
            Self::Host { id, .. } | Self::Remote { id, .. } => {
                bool::const_bit_length()   // is_host
                + bool::const_bit_length() // is_static
                + UnsignedVariableInteger::<7>::new(*id).bit_length()
            }
        }
    }

    pub(crate) fn convert_to_global(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match self {
            OwnedLocalEntity::Host { id, is_static: true } => {
                converter.static_host_entity_to_global_entity(&HostEntity::new(*id))
            }
            OwnedLocalEntity::Host { id, is_static: false } => {
                converter.host_entity_to_global_entity(&HostEntity::new(*id))
            }
            OwnedLocalEntity::Remote { id, is_static } => {
                let remote = if *is_static {
                    RemoteEntity::new_static(*id)
                } else {
                    RemoteEntity::new(*id)
                };
                converter.remote_entity_to_global_entity(&remote)
            }
        }
    }

    pub(crate) fn take_remote(&self) -> RemoteEntity {
        let OwnedLocalEntity::Remote { id, is_static } = self else {
            panic!("Expected RemoteEntity")
        };
        if *is_static { RemoteEntity::new_static(*id) } else { RemoteEntity::new(*id) }
    }

    pub(crate) fn to_reversed(self) -> OwnedLocalEntity {
        match self {
            OwnedLocalEntity::Host { id, is_static } => OwnedLocalEntity::Remote { id, is_static },
            OwnedLocalEntity::Remote { id, is_static } => OwnedLocalEntity::Host { id, is_static },
        }
    }

    /// Extracts the inner [`HostEntity`], panicking if this is a `Remote` variant.
    pub fn host(&self) -> HostEntity {
        match self {
            OwnedLocalEntity::Host { id, is_static } => {
                if *is_static { HostEntity::new_static(*id) } else { HostEntity::new(*id) }
            }
            OwnedLocalEntity::Remote { .. } => panic!("Expected OwnedLocalEntity::Host, found OwnedLocalEntity::Remote"),
        }
    }

    /// Extracts the inner [`RemoteEntity`], panicking if this is a `Host` variant.
    pub fn remote(&self) -> RemoteEntity {
        match self {
            OwnedLocalEntity::Remote { id, is_static } => {
                if *is_static { RemoteEntity::new_static(*id) } else { RemoteEntity::new(*id) }
            }
            OwnedLocalEntity::Host { .. } => panic!("Expected OwnedLocalEntity::Remote, found OwnedLocalEntity::Host"),
        }
    }
}

/// A host-assigned entity ID. Carries `is_static` so that static and dynamic
/// entities from pools that both start at 0 remain distinct as hash map keys.
#[derive(Copy, Eq, Hash, Clone, PartialEq, Debug)]
pub struct HostEntity {
    id: u16,
    is_static: bool,
}

impl HostEntity {
    /// Creates a dynamic host entity with the given `id`.
    pub fn new(id: u16) -> Self {
        Self { id, is_static: false }
    }

    /// Creates a static host entity with the given `id`.
    pub fn new_static(id: u16) -> Self {
        Self { id, is_static: true }
    }

    /// Returns the raw `u16` wire ID.
    pub fn value(&self) -> u16 {
        self.id
    }

    /// Returns `true` if this entity is from the static pool.
    pub fn is_static(&self) -> bool {
        self.is_static
    }

    /// Converts this host entity into the equivalent [`RemoteEntity`] with the same ID and static flag.
    pub fn to_remote(self) -> RemoteEntity {
        if self.is_static { RemoteEntity::new_static(self.id) } else { RemoteEntity::new(self.id) }
    }

    /// Serializes the entity ID into the bit stream (ID only; authority messages use dynamic entities).
    pub fn ser(&self, writer: &mut dyn BitWrite) {
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    /// Deserializes a dynamic host entity from the bit stream.
    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(Self { id: value as u16, is_static: false }) // authority messages only use dynamic entities
    }

    /// Returns the encoded bit length of this entity's ID.
    pub fn bit_length(&self) -> u32 {
        UnsignedVariableInteger::<7>::new(self.value()).bit_length()
    }

    /// Wraps this entity as an `OwnedLocalEntity::Host`, preserving the `is_static` flag.
    pub fn copy_to_owned(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Host { id: self.value(), is_static: self.is_static }
    }

    /// Wraps this entity as a dynamic `OwnedLocalEntity::Host` (forcing `is_static = false`).
    pub fn copy_to_owned_dynamic(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Host { id: self.value(), is_static: false }
    }

    /// Wraps this entity as a static `OwnedLocalEntity::Host` (forcing `is_static = true`).
    pub fn copy_to_owned_static(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Host { id: self.value(), is_static: true }
    }
}

/// A connection-local entity ID assigned by the remote peer, used on the receiving side of replication.
#[derive(Copy, Eq, Hash, Clone, PartialEq, Debug)]
pub struct RemoteEntity {
    id: u16,
    is_static: bool,
}

impl RemoteEntity {
    /// Creates a dynamic remote entity with the given `id`.
    pub fn new(id: u16) -> Self {
        Self { id, is_static: false }
    }

    /// Creates a static remote entity with the given `id`.
    pub fn new_static(id: u16) -> Self {
        Self { id, is_static: true }
    }

    /// Returns the raw `u16` wire ID.
    pub fn value(&self) -> u16 {
        self.id
    }

    /// Returns `true` if this entity is from the static pool.
    pub fn is_static(&self) -> bool {
        self.is_static
    }

    /// Converts this remote entity into the equivalent [`HostEntity`] with the same ID and static flag.
    pub fn to_host(self) -> HostEntity {
        if self.is_static { HostEntity::new_static(self.id) } else { HostEntity::new(self.id) }
    }

    /// Serializes only the entity ID into the bit stream (authority messages always use dynamic entities).
    pub fn ser(&self, writer: &mut dyn BitWrite) {
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    /// Deserializes a dynamic remote entity from the bit stream.
    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(Self { id: value as u16, is_static: false })
    }

    /// Wraps this entity as an `OwnedLocalEntity::Remote`, preserving the `is_static` flag.
    pub fn copy_to_owned(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Remote { id: self.id, is_static: self.is_static }
    }
}
