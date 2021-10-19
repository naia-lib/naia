use std::{
    any::TypeId,
    hash::Hash,
    ops::{Deref, DerefMut},
};

use super::{
    entity_type::EntityType,
    replicate::{Replicate, ReplicateEq},
};

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType: Sized + Sync + Send + Clone + 'static {
    type Kind: ProtocolKindType;

    /// Get kind of Replicate type
    fn kind_of<R: Replicate<Self>>() -> Self::Kind;
    /// Get kind from a type_id
    fn type_to_kind(type_id: TypeId) -> Self::Kind;
    /// Get an immutable reference to the inner Component/Message as a Replicate
    /// trait object
    fn dyn_ref(&self) -> DynRef<'_, Self>;
    /// Get an mutable reference to the inner Component/Message as a Replicate
    /// trait object
    fn dyn_mut(&mut self) -> DynMut<'_, Self>;
    /// Cast to a ReplicateEq impl
    fn cast<R: ReplicateEq<Self>>(self) -> Option<R>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: Replicate<Self>>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: Replicate<Self>>(&mut self) -> Option<&mut R>;
    /// Sets the current Protocol to the state of another Protocol of the
    /// same type
    fn mirror(&mut self, other: &Self);
    /// Extract an inner Replicate impl from the ProtocolType into a
    /// ProtocolExtractor impl
    fn extract_and_insert<N: EntityType, X: ProtocolExtractor<Self, N>>(
        &self,
        entity: &N,
        extractor: &mut X,
    );
}

pub trait ProtocolKindType: Eq + Hash + Copy {
    fn to_u16(&self) -> u16;
    fn from_u16(val: u16) -> Self;
}

pub trait ProtocolExtractor<P: ProtocolType, N: EntityType> {
    fn extract<R: Replicate<P>>(&mut self, entity: &N, component: R);
}

// DynRef

pub struct DynRef<'b, P: ProtocolType> {
    inner: &'b dyn Replicate<P>,
}

impl<'b, P: ProtocolType> DynRef<'b, P> {
    pub fn new(inner: &'b dyn Replicate<P>) -> Self {
        return Self { inner };
    }
}

impl<P: ProtocolType> Deref for DynRef<'_, P> {
    type Target = dyn Replicate<P>;

    #[inline]
    fn deref(&self) -> &dyn Replicate<P> {
        self.inner
    }
}

// DynMut

pub struct DynMut<'b, P: ProtocolType> {
    inner: &'b mut dyn Replicate<P>,
}

impl<'b, P: ProtocolType> DynMut<'b, P> {
    pub fn new(inner: &'b mut dyn Replicate<P>) -> Self {
        return Self { inner };
    }
}

impl<P: ProtocolType> Deref for DynMut<'_, P> {
    type Target = dyn Replicate<P>;

    #[inline]
    fn deref(&self) -> &dyn Replicate<P> {
        self.inner
    }
}

impl<P: ProtocolType> DerefMut for DynMut<'_, P> {
    #[inline]
    fn deref_mut(&mut self) -> &mut dyn Replicate<P> {
        self.inner
    }
}
