use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedVariableInteger};

// LocalEntity
#[derive(Copy, Eq, Hash, Clone, PartialEq, Debug)]
pub enum LocalEntity {
    Host(u16),
    Remote(u16),
}

impl LocalEntity {
    pub fn new_host(id: u16) -> Self {
        Self::Host(id)
    }

    pub fn new_remote(id: u16) -> Self {
        Self::Remote(id)
    }

    pub fn is_host(&self) -> bool {
        match self {
            LocalEntity::Host(_) => true,
            LocalEntity::Remote(_) => false,
        }
    }

    pub fn is_remote(&self) -> bool {
        match self {
            LocalEntity::Host(_) => false,
            LocalEntity::Remote(_) => true,
        }
    }

    pub fn value(&self) -> u16 {
        match self {
            LocalEntity::Host(value) => *value,
            LocalEntity::Remote(value) => *value,
        }
    }

    pub fn to_reversed(self) -> Self {
        match self {
            LocalEntity::Host(value) => LocalEntity::Remote(value),
            LocalEntity::Remote(value) => LocalEntity::Host(value),
        }
    }

    pub fn host_ser(&self, writer: &mut dyn BitWrite) {
        if !self.is_host() {
            panic!("Can only serialize LocalEntity::Host")
        }
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    pub fn host_bit_length(&self) -> u32 {
        UnsignedVariableInteger::<7>::new(self.value()).bit_length()
    }

    pub fn remote_de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        Ok(Self::Remote(value as u16))
    }

    pub fn owned_ser(&self, writer: &mut dyn BitWrite) {
        self.is_host().ser(writer);
        UnsignedVariableInteger::<7>::new(self.value()).ser(writer);
    }

    pub fn owned_de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let is_host = bool::de(reader)?;
        let value = UnsignedVariableInteger::<7>::de(reader)?.get();
        match is_host {
            true => Ok(Self::Host(value as u16)),
            false => Ok(Self::Remote(value as u16)),
        }
    }

    pub fn owned_bit_length(&self) -> u32 {
        bool::const_bit_length() + UnsignedVariableInteger::<7>::new(self.value()).bit_length()
    }
}
