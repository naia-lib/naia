use std::{
    any::TypeId,
    hash::Hash,
    ops::{Deref, DerefMut},
};

use super::{
    entity_type::EntityType,
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
    fn dyn_ref(&self) -> DynRef<'_, Self>;
    /// Get an mutable reference to the inner Component/Message as a
    /// ReplicateSafe trait object
    fn dyn_mut(&mut self) -> DynMut<'_, Self>;
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

pub trait ProtocolRefType<P: ProtocolType> {
    fn as_dyn<'a>(&self) -> DynRef<'a, P>;
}
pub trait ProtocolMutType<P: ProtocolType> {
    fn as_dyn<'a>(&mut self) -> DynMut<'a, P>;
}

pub trait ProtocolKindType: Eq + Hash + Copy {
    fn to_u16(&self) -> u16;
    fn from_u16(val: u16) -> Self;
}

pub trait ProtocolInserter<P: ProtocolType, N: EntityType> {
    fn insert<R: ReplicateSafe<P>>(&mut self, entity: &N, component: R);
}

// DynRef

pub struct DynRef<'b, P: ProtocolType> {
    inner: &'b dyn ReplicateSafe<P>,
}

impl<'b, P: ProtocolType> DynRef<'b, P> {
    pub fn new(inner: &'b dyn ReplicateSafe<P>) -> Self {
        return Self { inner };
    }
}

impl<P: ProtocolType> Deref for DynRef<'_, P> {
    type Target = dyn ReplicateSafe<P>;

    #[inline]
    fn deref(&self) -> &dyn ReplicateSafe<P> {
        self.inner
    }
}

// DynMut

pub struct DynMut<'b, P: ProtocolType> {
    inner: &'b mut dyn ReplicateSafe<P>,
}

impl<'b, P: ProtocolType> DynMut<'b, P> {
    pub fn new(inner: &'b mut dyn ReplicateSafe<P>) -> Self {
        return Self { inner };
    }
}

impl<P: ProtocolType> Deref for DynMut<'_, P> {
    type Target = dyn ReplicateSafe<P>;

    #[inline]
    fn deref(&self) -> &dyn ReplicateSafe<P> {
        self.inner
    }
}

impl<P: ProtocolType> DerefMut for DynMut<'_, P> {
    #[inline]
    fn deref_mut(&mut self) -> &mut dyn ReplicateSafe<P> {
        self.inner
    }
}
