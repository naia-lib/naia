use std::ops::{Deref, DerefMut};

use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::protocol::property_mutate::PropertyMutator;
use crate::protocol::replicable_property::ReplicableProperty;

/// A Property of an Component/Message, that contains data
/// which must be tracked for updates
#[derive(Clone)]
pub struct Property<T: Serde> {
    inner: T,
    mutator: Option<PropertyMutator>,
    mutator_index: u8,
}

// should be shared
impl<T: Serde> Property<T> {
    fn read_inner(reader: &mut BitReader) -> Result<T, SerdeErr> {
        T::de(reader)
    }
}

// should be shared
impl<T: Serde> ReplicableProperty for Property<T> {
    type Inner = T;

    /// Create a new Property
    fn new(value: Self::Inner, mutator_index: u8) -> Self {
        Property::<T> {
            inner: value,
            mutator: None,
            mutator_index,
        }
    }

    /// Set value to the value of another Property, queues for update if value
    /// changes
    fn mirror(&mut self, other: &Self) {
        **self = (**other).clone();
    }

    // Serialization / deserialization

    /// Writes contained value into outgoing byte stream
    fn write(&self, writer: &mut dyn BitWrite) {
        self.inner.ser(writer);
    }

    /// Given a cursor into incoming packet data, initializes the Property with
    /// the synced value
    fn new_read(reader: &mut BitReader, mutator_index: u8) -> Result<Self, SerdeErr> {
        let inner = Self::read_inner(reader)?;

        Ok(Property::<T> {
            inner,
            mutator: None,
            mutator_index,
        })
    }

    /// Reads from a stream and immediately writes to a stream
    /// Used to buffer updates for later
    fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        T::de(reader)?.ser(writer);
        Ok(())
    }

    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        self.inner = Self::read_inner(reader)?;
        Ok(())
    }

    // Comparison

    /// Compare to another property
    fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    // Internal

    /// Set an PropertyMutator to track changes to the Property
    fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.mutator = Some(mutator.clone_new());
    }
}

// It could be argued that Property here is a type of smart-pointer,
// but honestly this is mainly for the convenience of type coercion
impl<T: Serde> Deref for Property<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Serde> DerefMut for Property<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Just assume inner value will be changed, queue for update
        if let Some(mutator) = &mut self.mutator {
            mutator.mutate(self.mutator_index);
        }
        &mut self.inner
    }
}
