use std::ops::{Deref, DerefMut};

use naia_serde::{BitReader, BitWrite, BitWriter, Serde};

use crate::protocol::property_mutate::PropertyMutator;

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
    /// Create a new Property
    pub fn new(value: T, mutator_index: u8) -> Property<T> {
        Property::<T> {
            inner: value,
            mutator: None,
            mutator_index,
        }
    }

    /// Set value to the value of another Property, queues for update if value
    /// changes
    pub fn mirror(&mut self, other: &Property<T>) {
        **self = (**other).clone();
    }

    // Serialization / deserialization

    /// Writes contained value into outgoing byte stream
    pub fn write(&self, writer: &mut dyn BitWrite) {
        self.inner.ser(writer);
    }

    /// Given a cursor into incoming packet data, initializes the Property with
    /// the synced value
    pub fn new_read(reader: &mut BitReader, mutator_index: u8) -> Self {
        let inner = Self::read_inner(reader);

        Property::<T> {
            inner,
            mutator: None,
            mutator_index,
        }
    }

    /// Reads from a stream and immediately writes to a stream
    /// Used to buffer updates for later
    pub fn read_write(bit_reader: &mut BitReader, bit_writer: &mut BitWriter) {
        T::de(bit_reader)
            .expect("Property read error.")
            .ser(bit_writer);
    }

    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    pub fn read(&mut self, reader: &mut BitReader) {
        self.inner = Self::read_inner(reader);
    }

    fn read_inner(reader: &mut BitReader) -> T {
        T::de(reader).expect("Property read error.")
    }

    // Comparison

    /// Compare to another property
    pub fn equals(&self, other: &Property<T>) -> bool {
        self.inner == other.inner
    }

    // Internal

    /// Set an PropertyMutator to track changes to the Property
    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
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
