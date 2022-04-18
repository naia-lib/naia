use std::{any::TypeId, hash::Hash};

use naia_serde::{BitReader, BitWrite, Serde};

use crate::{protocol::component_update::ComponentUpdate, DiffMask, NetEntityHandleConverter};

use super::{
    replica_ref::{ReplicaDynMut, ReplicaDynRef},
    replicate::{Replicate, ReplicateSafe},
};

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait Protocolize: Clone + Sized + Sync + Send + 'static {
    type Kind: ProtocolKindType;

    /// Get kind of Replicate type
    fn kind_of<R: ReplicateSafe<Self>>() -> Self::Kind;
    /// Get kind from a type_id
    fn type_to_kind(type_id: TypeId) -> Option<Self::Kind>;
    /// Read from a bit stream to create a new Replica
    fn read(bit_reader: &mut BitReader, converter: &dyn NetEntityHandleConverter) -> Self;
    /// Read from a bit stream to create a new Component Update
    fn read_create_update(bit_reader: &mut BitReader) -> ComponentUpdate<Self::Kind>;
    /// Get an immutable reference to the inner Component/Message as a
    /// Replicate trait object
    fn dyn_ref(&self) -> ReplicaDynRef<'_, Self>;
    /// Get an mutable reference to the inner Component/Message as a
    /// Replicate trait object
    fn dyn_mut(&mut self) -> ReplicaDynMut<'_, Self>;
    /// Cast to a Replicate impl
    fn cast<R: Replicate<Self>>(self) -> Option<R>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: ReplicateSafe<Self>>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: ReplicateSafe<Self>>(&mut self) -> Option<&mut R>;
    /// Extract an inner Replicate impl from the Protocolize into a
    /// ProtocolInserter impl
    fn extract_and_insert<N, X: ProtocolInserter<Self, N>>(&self, entity: &N, inserter: &mut X);
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Message/Component on the client
    fn write(&self, bit_writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Message/Component on the client
    fn write_update(
        &self,
        diff_mask: &DiffMask,
        bit_writer: &mut dyn BitWrite,
        converter: &dyn NetEntityHandleConverter,
    );
}

pub trait ProtocolKindType: Eq + Hash + Copy + Send + Sync + Serde {
    fn to_type_id(&self) -> TypeId;
}

pub trait ProtocolInserter<P: Protocolize, N> {
    fn insert<R: ReplicateSafe<P>>(&mut self, entity: &N, component: R);
}
