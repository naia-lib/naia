use std::ops::{Deref, DerefMut};

use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::world::{component::property_mutate::PropertyMutator, delegation::{auth_channel::EntityAuthAccessor, entity_auth_status::EntityAuthStatus}};

#[derive(Clone)]
enum PropertyImpl<T: Serde> {
    HostOwned(HostOwnedProperty<T>),
    RemoteOwned(RemoteOwnedProperty<T>),
    RemotePublic(RemotePublicProperty<T>),
    Delegated(DelegatedProperty<T>),
    Local(LocalProperty<T>),
}

/// A Property of an Component/Message, that contains data
/// which must be tracked for updates
#[derive(Clone)]
pub struct Property<T: Serde> {
    inner: PropertyImpl<T>,
}

// should be shared
impl<T: Serde> Property<T> {
    /// Create a new Local Property
    pub fn new_local(value: T) -> Self {
        Self {
            inner: PropertyImpl::Local(LocalProperty::new(value)),
        }
    }

    /// Create a new host-owned Property
    pub fn host_owned(value: T, mutator_index: u8) -> Self {
        Self {
            inner: PropertyImpl::HostOwned(HostOwnedProperty::new(value, mutator_index)),
        }
    }

    /// Given a cursor into incoming packet data, initializes the Property with
    /// the synced value
    pub fn new_read(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let inner_value = Self::read_inner(reader)?;

        Ok(Self {
            inner: PropertyImpl::RemoteOwned(RemoteOwnedProperty::new(inner_value)),
        })
    }

    /// Set an PropertyMutator to track changes to the Property
    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                inner.set_mutator(mutator);
            }
            PropertyImpl::RemoteOwned(_) | PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never call set_mutator().");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never call set_mutator().");
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never have a mutator.");
            }
        }
    }

    // Serialization / deserialization

    /// Writes contained value into outgoing byte stream
    pub fn write(&self, writer: &mut dyn BitWrite) {
        match &self.inner {
            PropertyImpl::HostOwned(inner) => {
                inner.write(writer);
            }
            PropertyImpl::RemoteOwned(_) => {
                panic!("Remote Property should never be written.");
            }
            PropertyImpl::RemotePublic(inner) => {
                inner.write(writer);
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never be written.");
            }
            PropertyImpl::Delegated(inner) => {
                inner.write(writer);
            }
        }
    }

    /// Reads from a stream and immediately writes to a stream
    /// Used to buffer updates for later
    pub fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        T::de(reader)?.ser(writer);
        Ok(())
    }

    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    pub fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        match &mut self.inner {
            PropertyImpl::HostOwned(_) => {
                panic!("Host Property should never read.");
            }
            PropertyImpl::RemoteOwned(inner) => {
                inner.read(reader)?;
            }
            PropertyImpl::RemotePublic(inner) => {
                inner.read(reader)?;
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never read.");
            }
            PropertyImpl::Delegated(inner) => {
                inner.read(reader)?;
            }
        }
        Ok(())
    }

    fn read_inner(reader: &mut BitReader) -> Result<T, SerdeErr> {
        T::de(reader)
    }

    // Comparison

    fn inner(&self) -> &T {
        match &self.inner {
            PropertyImpl::HostOwned(inner) => &inner.inner,
            PropertyImpl::RemoteOwned(inner) => &inner.inner,
            PropertyImpl::RemotePublic(inner) => &inner.inner,
            PropertyImpl::Local(inner) => &inner.inner,
            PropertyImpl::Delegated(inner) => &inner.inner,
        }
    }

    /// Compare to another property
    pub fn equals(&self, other: &Self) -> bool {
        self.inner() == other.inner()
    }

    /// Set value to the value of another Property, queues for update if value
    /// changes
    pub fn mirror(&mut self, other: &Self) {
        let other_inner = other.inner();
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                inner.mirror(other_inner);
            }
            PropertyImpl::RemoteOwned(_) | PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never be set manually.");
            }
            PropertyImpl::Delegated(inner) => {
                inner.mirror(other_inner);
            }
            PropertyImpl::Local(inner) => {
                inner.mirror(other_inner);
            }
        }
    }

    /// Migrate Remote Property to Public version
    pub fn remote_publish(&mut self, mutator_index: u8, mutator: &PropertyMutator) {
        match &mut self.inner {
            PropertyImpl::HostOwned(_) => {
                panic!("Host Property should never be made public.");
            }
            PropertyImpl::RemoteOwned(inner) => {
                let inner_value = inner.inner.clone();
                self.inner = PropertyImpl::RemotePublic(RemotePublicProperty::new(
                    inner_value,
                    mutator_index,
                    mutator,
                ));
            }
            PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never be made public twice.");
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never be made public.");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never be made public.");
            }
        }
    }

    /// Migrate Remote Property to Private version
    pub fn remote_unpublish(&mut self) {
        match &mut self.inner {
            PropertyImpl::HostOwned(_) => {
                panic!("Host Property should never be unpublished.");
            }
            PropertyImpl::RemoteOwned(_) => {
                panic!("Private Remote Property should never be unpublished.");
            }
            PropertyImpl::RemotePublic(inner) => {
                let inner_value = inner.inner.clone();
                self.inner = PropertyImpl::RemoteOwned(RemoteOwnedProperty::new(inner_value));
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never be unpublished.");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never be unpublished.");
            }
        }
    }

    /// Migrate Host/RemotePublic Property to Delegated version
    pub fn enable_delegation(&mut self, accessor: &EntityAuthAccessor) {
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                self.inner = PropertyImpl::Delegated(DelegatedProperty::new(
                    inner.inner.clone(),
                    inner.index,
                    inner.mutator.as_ref().unwrap(),
                    accessor,
                ));
            }
            PropertyImpl::RemoteOwned(_) => {
                panic!("Private Remote Property should never enable delegation.");
            }
            PropertyImpl::RemotePublic(inner) => {
                self.inner = PropertyImpl::Delegated(DelegatedProperty::new(
                    inner.inner.clone(),
                    inner.index,
                    &inner.mutator,
                    accessor,
                ));
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never enable delegation.");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never enable delegation twice.");
            }
        }
    }

    /// Migrate Delegated Property to Host-Owned (Public) version
    pub fn disable_delegation(&mut self) {
        match &mut self.inner {
            PropertyImpl::HostOwned(_) => {
                panic!("Host Property should never disable delegation.");
            }
            PropertyImpl::RemoteOwned(_) => {
                panic!("Private Remote Property should never disable delegation.");
            }
            PropertyImpl::RemotePublic(_) => {
                panic!("Public Remote Property should never disable delegation.");
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never disable delegation.");
            }
            PropertyImpl::Delegated(inner) => {
                let inner_value = inner.inner.clone();
                let mut new_inner = HostOwnedProperty::new(inner_value, inner.index);
                new_inner.set_mutator(&inner.mutator);
                self.inner = PropertyImpl::HostOwned(new_inner);
            }
        }
    }

    /// Migrate Host Property to Local version
    pub fn localize(&mut self) {
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                let inner_value = inner.inner.clone();
                self.inner = PropertyImpl::Local(LocalProperty::new(inner_value));
            }
            PropertyImpl::RemoteOwned(_) | PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never be made local.");
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never be made local twice.");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never be made local.");
            }
        }
    }
}

// It could be argued that Property here is a type of smart-pointer,
// but honestly this is mainly for the convenience of type coercion
impl<T: Serde> Deref for Property<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl<T: Serde> DerefMut for Property<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Just assume inner value will be changed, queue for update
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                inner.mutate();
                &mut inner.inner
            }
            PropertyImpl::RemoteOwned(_) | PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never be set manually.");
            }
            PropertyImpl::Local(inner) => &mut inner.inner,
            PropertyImpl::Delegated(inner) => {
                if !inner.can_mutate() {
                    panic!("Must request authority to mutate a Delegated Property.");
                }
                inner.mutate();
                &mut inner.inner
            }
        }
    }
}

#[derive(Clone)]
pub struct HostOwnedProperty<T: Serde> {
    inner: T,
    mutator: Option<PropertyMutator>,
    index: u8,
}

impl<T: Serde> HostOwnedProperty<T> {
    /// Create a new HostOwnedProperty
    pub fn new(value: T, mutator_index: u8) -> Self {
        Self {
            inner: value,
            mutator: None,
            index: mutator_index,
        }
    }

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.mutator = Some(mutator.clone_new());
    }

    pub fn write(&self, writer: &mut dyn BitWrite) {
        self.inner.ser(writer);
    }

    pub fn mirror(&mut self, other: &T) {
        self.mutate();
        self.inner = other.clone();
    }

    pub fn mutate(&mut self) {
        let Some(mutator) = &mut self.mutator else {
            panic!("Host Property should have a mutator immediately after creation.");
        };
        mutator.mutate(self.index);
    }
}

#[derive(Clone)]
pub struct LocalProperty<T: Serde> {
    inner: T,
}

impl<T: Serde> LocalProperty<T> {
    /// Create a new LocalProperty
    pub fn new(value: T) -> Self {
        Self { inner: value }
    }

    pub fn mirror(&mut self, other: &T) {
        self.inner = other.clone();
    }
}

#[derive(Clone)]
pub struct RemoteOwnedProperty<T: Serde> {
    inner: T,
}

impl<T: Serde> RemoteOwnedProperty<T> {
    /// Create a new RemoteOwnedProperty
    pub fn new(value: T) -> Self {
        Self { inner: value }
    }

    pub fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        self.inner = Property::read_inner(reader)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct RemotePublicProperty<T: Serde> {
    inner: T,
    mutator: PropertyMutator,
    index: u8,
}

impl<T: Serde> RemotePublicProperty<T> {
    /// Create a new RemotePublicProperty
    pub fn new(value: T, mutator_index: u8, mutator: &PropertyMutator) -> Self {
        Self {
            inner: value,
            mutator: mutator.clone_new(),
            index: mutator_index,
        }
    }

    pub fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        self.inner = Property::read_inner(reader)?;
        self.mutate();
        Ok(())
    }

    pub fn write(&self, writer: &mut dyn BitWrite) {
        self.inner.ser(writer);
    }

    fn mutate(&mut self) {
        self.mutator.mutate(self.index);
    }
}

#[derive(Clone)]
pub struct DelegatedProperty<T: Serde> {
    inner: T,
    mutator: PropertyMutator,
    auth_accessor: EntityAuthAccessor,
    index: u8,
}

impl<T: Serde> DelegatedProperty<T> {
    /// Create a new DelegatedProperty
    pub fn new(
        value: T,
        index: u8,
        mutator: &PropertyMutator,
        auth_accessor: &EntityAuthAccessor,
    ) -> Self {
        Self {
            inner: value,
            mutator: mutator.clone_new(),
            auth_accessor: auth_accessor.clone(),
            index,
        }
    }

    pub fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        if !self.can_read() {
            panic!("Should only read Delegated Property if we do not have authority.");
        }
        self.inner = Property::read_inner(reader)?;
        Ok(())
    }

    pub fn write(&self, writer: &mut dyn BitWrite) {
        if !self.can_write() {
            panic!("Must have Authority over Entity before performing this operation.");
        }
        self.inner.ser(writer);
    }

    pub fn mirror(&mut self, other: &T) {
        if !self.can_mutate() {
            panic!("Request Authority over Entity before performing this operation.");
        }
        self.mutate();
        self.inner = other.clone();
    }

    fn mutate(&mut self) {
        self.mutator.mutate(self.index);
    }

    fn can_mutate(&self) -> bool {
        match self.auth_accessor.auth_status() {
            EntityAuthStatus::AvailableAuthority => false,
            EntityAuthStatus::RequestedAuthority => true,
            EntityAuthStatus::HasAuthority => true,
            EntityAuthStatus::NoAuthority => false,
        }
    }

    fn can_read(&self) -> bool {
        match self.auth_accessor.auth_status() {
            EntityAuthStatus::AvailableAuthority => true,
            EntityAuthStatus::RequestedAuthority => false,
            EntityAuthStatus::HasAuthority => false,
            EntityAuthStatus::NoAuthority => true,
        }
    }

    fn can_write(&self) -> bool {
        match self.auth_accessor.auth_status() {
            EntityAuthStatus::AvailableAuthority => false,
            EntityAuthStatus::RequestedAuthority => false,
            EntityAuthStatus::HasAuthority => true,
            EntityAuthStatus::NoAuthority => false,
        }
    }
}
