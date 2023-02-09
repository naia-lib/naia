use naia_serde::{BitReader, OwnedBitReader};

use crate::component::component_kinds::ComponentKind;

pub struct ComponentUpdate {
    pub kind: ComponentKind,
    buffer: OwnedBitReader,
}

impl ComponentUpdate {
    pub fn new(kind: ComponentKind, buffer: OwnedBitReader) -> Self {
        Self { kind, buffer }
    }

    pub fn reader(&self) -> BitReader {
        self.buffer.borrow()
    }
}
