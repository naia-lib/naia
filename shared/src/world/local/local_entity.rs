use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedVariableInteger};

use crate::{EntityDoesNotExistError, GlobalEntity, LocalEntityAndGlobalEntityConverter};

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum OwnedLocalEntity {
    Host { id: u16, is_static: bool },
    Remote(u16),
}

impl OwnedLocalEntity {
    pub fn new_host(id: HostEntity) -> Self {
        Self::Host { id: id.value(), is_static: false }
    }

    pub fn new_host_dynamic(id: u16) -> Self {
        Self::Host { id, is_static: false }
    }

    pub fn new_host_static(id: u16) -> Self {
        Self::Host { id, is_static: true }
    }

    pub fn new_remote(id: RemoteEntity) -> Self {
        Self::Remote(id.value())
    }

    pub fn is_host(&self) -> bool {
        match self {
            Self::Host { .. } => true,
            Self::Remote(_) => false,
        }
    }

    pub fn is_remote(&self) -> bool {
        !self.is_host()
    }

    pub fn is_static(&self) -> bool {
        match self {
            Self::Host { is_static, .. } => *is_static,
            Self::Remote(_) => false,
        }
    }

    pub(crate) fn value(&self) -> u16 {
        match self {
            Self::Host { id, .. } => *id,
            Self::Remote(value) => *value,
        }
    }

    pub fn ser(&self, writer: &mut dyn BitWrite) {
        match self {
            Self::Host { id, is_static } => {
                true.ser(writer);
                is_static.ser(writer);
                UnsignedVariableInteger::<7>::new(*id).ser(writer);
            }
            Self::Remote(id) => {
                false.ser(writer);
                UnsignedVariableInteger::<7>::new(*id).ser(writer);
            }
        }
    }

    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let is_host = bool::de(reader)?;
        if is_host {
            let is_static = bool::de(reader)?;
            let id = UnsignedVariableInteger::<7>::de(reader)?.get() as u16;
            Ok(Self::Host { id, is_static })
        } else {
            let id = UnsignedVariableInteger::<7>::de(reader)?.get() as u16;
            Ok(Self::Remote(id))
        }
    }

    pub fn bit_length(&self) -> u32 {
        match self {
            Self::Host { id, .. } => {
                bool::const_bit_length()   // is_host
                + bool::const_bit_length() // is_static
                + UnsignedVariableInteger::<7>::new(*id).bit_length()
            }
            Self::Remote(id) => {
                bool::const_bit_length()   // is_host
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
            OwnedLocalEntity::Remote(remote_entity) => {
                converter.remote_entity_to_global_entity(&RemoteEntity::new(*remote_entity))
            }
        }
    }

    pub(crate) fn take_remote(&self) -> RemoteEntity {
        let OwnedLocalEntity::Remote(remote_entity) = self else {
            panic!("Expected RemoteEntity")
        };
        RemoteEntity::new(*remote_entity)
    }

    pub(crate) fn to_reversed(&self) -> OwnedLocalEntity {
        match self {
            OwnedLocalEntity::Host { id, .. } => OwnedLocalEntity::Remote(*id),
            OwnedLocalEntity::Remote(remote_entity) => OwnedLocalEntity::Host { id: *remote_entity, is_static: false },
        }
    }

    pub fn host(&self) -> HostEntity {
        match self {
            OwnedLocalEntity::Host { id, is_static } => {
                if *is_static { HostEntity::new_static(*id) } else { HostEntity::new(*id) }
            }
            OwnedLocalEntity::Remote(_) => panic!("Expected OwnedLocalEntity::Host, found OwnedLocalEntity::Remote"),
        }
    }

    pub fn remote(&self) -> RemoteEntity {
        if !self.is_remote() {
            panic!("Expected OwnedLocalEntity::Remote, found OwnedLocalEntity::Host");
        }
        RemoteEntity::new(self.value())
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
    pub fn new(id: u16) -> Self {
        Self { id, is_static: false }
    }

    pub fn new_static(id: u16) -> Self {
        Self { id, is_static: true }
    }

    pub fn value(&self) -> u16 {
        self.id
    }

    pub fn is_static(&self) -> bool {
        self.is_static
    }

    pub fn to_remote(self) -> RemoteEntity {
        RemoteEntity::new(self.id)
    }

    pub fn ser(&self, writer: &mut dyn BitWrite) {
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(Self { id: value as u16, is_static: false }) // authority messages only use dynamic entities
    }

    pub fn bit_length(&self) -> u32 {
        UnsignedVariableInteger::<7>::new(self.value()).bit_length()
    }

    pub fn copy_to_owned(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Host { id: self.value(), is_static: self.is_static }
    }

    pub fn copy_to_owned_dynamic(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Host { id: self.value(), is_static: false }
    }

    pub fn copy_to_owned_static(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Host { id: self.value(), is_static: true }
    }
}

// RemoteEntity
#[derive(Copy, Eq, Hash, Clone, PartialEq, Debug)]
pub struct RemoteEntity(u16);

impl RemoteEntity {
    pub fn new(id: u16) -> Self {
        Self(id)
    }

    pub fn value(&self) -> u16 {
        self.0
    }

    pub fn to_host(self) -> HostEntity {
        HostEntity::new(self.0)
    }

    pub fn ser(&self, writer: &mut dyn BitWrite) {
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(Self(value as u16))
    }

    pub fn copy_to_owned(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Remote(self.value())
    }
}
