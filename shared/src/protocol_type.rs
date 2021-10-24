use std::{any::TypeId, hash::Hash};

use super::{
    entity_type::EntityType,
    replica_ref::{ReplicaDynMut, ReplicaDynRef},
    replicate::{Replicate, ReplicateSafe},
};

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType: Sized + Sync + Send + Clone + 'static {
    type Kind: ProtocolKindType;

    /// Get kind of ReplicateSafe type
    fn kind_of<R: ReplicateSafe<Self>>() -> Self::Kind;
    /// Get kind from a type_id
    fn type_to_kind(type_id: TypeId) -> Self::Kind;
    /// Get an immutable reference to the inner Component/Message as a
    /// ReplicateSafe trait object
    fn dyn_ref(&self) -> ReplicaDynRef<'_, Self>;
    /// Get an mutable reference to the inner Component/Message as a
    /// ReplicateSafe trait object
    fn dyn_mut(&mut self) -> ReplicaDynMut<'_, Self>;
    /// Cast to a ReplicateSafe impl
    fn cast<R: Replicate<Self>>(self) -> Option<R>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: ReplicateSafe<Self>>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: ReplicateSafe<Self>>(&mut self) -> Option<&mut R>;
    /// Extract an inner ReplicateSafe impl from the ProtocolType into a
    /// ProtocolInserter impl
    fn extract_and_insert<N: EntityType, X: ProtocolInserter<Self, N>>(
        &self,
        entity: &N,
        inserter: &mut X,
    );
}

pub trait ProtocolKindType: Eq + Hash + Copy {
    fn to_u16(&self) -> u16;
    fn from_u16(val: u16) -> Self;
}

pub trait ProtocolInserter<P: ProtocolType, N: EntityType> {
    fn insert<R: ReplicateSafe<P>>(&mut self, entity: &N, component: R);
}
