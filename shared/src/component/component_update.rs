use naia_serde::{BitReader, OwnedBitReader};

use crate::types::ComponentId;

pub struct ComponentUpdate {
    pub kind: ComponentId,
    buffer: OwnedBitReader,
}

impl ComponentUpdate {
    pub fn new(kind: ComponentId, buffer: OwnedBitReader) -> Self {
        Self { kind, buffer }
    }

    pub fn reader(&self) -> BitReader {
        self.buffer.borrow()
    }
}
