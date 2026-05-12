use naia_serde::{BitReader, OwnedBitReader};

use crate::{
    world::component::component_kinds::ComponentKind,
    world::component::replicate::SplitUpdateResult,
    ComponentKinds,
    LocalEntityAndGlobalEntityConverter,
};

/// A serialised component-field update payload together with its [`ComponentKind`] tag.
pub struct ComponentUpdate {
    /// The kind of component this update applies to.
    pub kind: ComponentKind,
    buffer: OwnedBitReader,
}

impl ComponentUpdate {
    /// Creates a new `ComponentUpdate` wrapping `buffer` for the given `kind`.
    pub fn new(kind: ComponentKind, buffer: OwnedBitReader) -> Self {
        Self { kind, buffer }
    }

    /// Borrows the payload buffer as a [`BitReader`] for field deserialization.
    pub fn reader(&'_ self) -> BitReader<'_> {
        self.buffer.borrow()
    }

    pub(crate) fn split_into_waiting_and_ready(
        self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        component_kinds: &ComponentKinds,
    ) -> SplitUpdateResult {
        let kind = self.kind;
        component_kinds.split_update(converter, &kind, self)
    }
}

/// A single serialised field update payload for a component, identified by field index.
pub struct ComponentFieldUpdate {
    id: u8,
    buffer: OwnedBitReader,
}

impl ComponentFieldUpdate {
    /// Creates a `ComponentFieldUpdate` for field `id` with the given serialized `buffer`.
    pub fn new(id: u8, buffer: OwnedBitReader) -> Self {
        Self { id, buffer }
    }

    /// Returns the field index this update targets.
    pub fn field_id(&self) -> u8 {
        self.id
    }

    /// Borrows the field payload as a [`BitReader`] for deserialization.
    pub fn reader(&'_ self) -> BitReader<'_> {
        self.buffer.borrow()
    }
}
