use std::ops::{Deref, DerefMut};

use naia_socket_shared::PacketReader;
use naia_serde::{BitReader, Serde, SerdeErr};

use crate::property_mutate::PropertyMutator;

/// A Property of an Component/Message, that contains data
/// which must be tracked for updates
#[derive(Clone)]
pub struct Property<T: Serde> {
    inner: T,
    mutator: Option<PropertyMutator>,
    mutator_index: u8,
}

impl<T: Serde> Serde for Property<T> {
    fn ser<S: BitWrite>(&self, writer: &mut S) {
        self.inner.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        todo!()
    }
}

// should be shared
impl<T: Serde> Property<T> {
    /// Create a new Property
    pub fn new(value: T, mutator_index: u8) -> Property<T> {
        return Property::<T> {
            inner: value,
            mutator: None,
            mutator_index,
        };
    }

    /// Set value to the value of another Property, queues for update if value
    /// changes
    pub fn mirror(&mut self, other: &Property<T>) {
        **self = (**other).clone();
    }

    // Serialization / deserialization

    /// Writes contained value into outgoing byte stream
    pub fn write(&self, buffer: &mut Vec<u8>) {
        let encoded = &mut SerBin::serialize_bin(&self.inner);
        buffer.push(encoded.len() as u8);
        buffer.append(encoded);
    }

    /// Returns the number of bytes used to encode / decode the Property
    pub fn size() -> usize {
        std::mem::size_of::<T>() + 1
    }

    /// Given a cursor into incoming packet data, initializes the Property with
    /// the synced value
    pub fn new_read(reader: &mut PacketReader, mutator_index: u8) -> Self {
        let inner = Self::read_inner(reader);

        return Property::<T> {
            inner,
            mutator: None,
            mutator_index,
        };
    }

    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    pub fn read(&mut self, reader: &mut PacketReader) {
        self.inner = Self::read_inner(reader);
    }

    fn read_inner(reader: &mut PacketReader) -> T {
        let length = reader.read_u8();

        let buffer = reader.buffer();
        let cursor = reader.cursor();

        let start: usize = cursor.position() as usize;
        let end: usize = start + (length as usize);

        let inner =
            DeBin::deserialize_bin(&buffer[start..end]).expect("error deserializing property");

        cursor.set_position(end as u64);

        return inner;
    }

    // Comparison

    /// Compare to another property
    pub fn equals(&self, other: &Property<T>) -> bool {
        return self.inner == other.inner;
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
        return &mut self.inner;
    }
}
