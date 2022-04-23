use naia_serde::{BitReader, OwnedBitReader};

use super::protocolize::ProtocolKindType;

pub struct ComponentUpdate<K: ProtocolKindType> {
    pub kind: K,
    buffer: OwnedBitReader,
}

impl<K: ProtocolKindType> ComponentUpdate<K> {
    pub fn new(kind: K, buffer: OwnedBitReader) -> Self {
        Self { kind, buffer }
    }

    pub fn reader(&self) -> BitReader {
        self.buffer.borrow()
    }
}
