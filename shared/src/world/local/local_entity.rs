use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedVariableInteger};

use crate::{EntityDoesNotExistError, GlobalEntity, LocalEntityAndGlobalEntityConverter};

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum OwnedLocalEntity {
    Host { id: u16, is_static: bool },
    Remote { id: u16, is_static: bool },
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
        Self::Remote { id: id.value(), is_static: id.is_static() }
    }

    pub fn new_remote_dynamic(id: u16) -> Self {
        Self::Remote { id, is_static: false }
    }

    pub fn new_remote_static(id: u16) -> Self {
        Self::Remote { id, is_static: true }
    }

    pub fn is_host(&self) -> bool {
        match self {
            Self::Host { .. } => true,
            Self::Remote { .. } => false,
        }
    }

    pub fn is_remote(&self) -> bool {
        !self.is_host()
    }

    pub fn is_static(&self) -> bool {
        match self {
            Self::Host { is_static, .. } => *is_static,
            Self::Remote { is_static, .. } => *is_static,
        }
    }

    pub fn id(&self) -> u16 {
        match self {
            Self::Host { id, .. } | Self::Remote { id, .. } => *id,
        }
    }

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

    pub fn host(&self) -> HostEntity {
        match self {
            OwnedLocalEntity::Host { id, is_static } => {
                if *is_static { HostEntity::new_static(*id) } else { HostEntity::new(*id) }
            }
            OwnedLocalEntity::Remote { .. } => panic!("Expected OwnedLocalEntity::Host, found OwnedLocalEntity::Remote"),
        }
    }

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
        if self.is_static { RemoteEntity::new_static(self.id) } else { RemoteEntity::new(self.id) }
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
pub struct RemoteEntity {
    id: u16,
    is_static: bool,
}

impl RemoteEntity {
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

    pub fn to_host(self) -> HostEntity {
        if self.is_static { HostEntity::new_static(self.id) } else { HostEntity::new(self.id) }
    }

    // Writes only the ID — used for authority messages which are always dynamic.
    pub fn ser(&self, writer: &mut dyn BitWrite) {
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    // Reads only the ID and produces a dynamic RemoteEntity — used for authority messages.
    pub fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(Self { id: value as u16, is_static: false })
    }

    pub fn copy_to_owned(&self) -> OwnedLocalEntity {
        OwnedLocalEntity::Remote { id: self.id, is_static: self.is_static }
    }
}
