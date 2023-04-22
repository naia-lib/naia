
use naia_serde::{BitReader, OwnedBitReader, SerdeErr};

use crate::{
    world::component::component_kinds::ComponentKind, ComponentKinds, LocalEntity,
    LocalEntityAndGlobalEntityConverter,
};

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

    pub(crate) fn split_into_waiting_and_ready(
        self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        component_kinds: &ComponentKinds,
    ) -> Result<(Option<Vec<(LocalEntity, ComponentFieldUpdate)>>, Option<Self>), SerdeErr> {
        let kind = self.kind;
        component_kinds.split_update(converter, &kind, self)
    }
}

pub struct ComponentFieldUpdate {
    id: u8,
    buffer: OwnedBitReader,
}

impl ComponentFieldUpdate {
    pub fn new(id: u8, buffer: OwnedBitReader) -> Self {
        Self { id, buffer }
    }

    pub fn field_id(&self) -> u8 {
        self.id
    }

    pub fn reader(&self) -> BitReader {
        self.buffer.borrow()
    }
}