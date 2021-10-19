use nanoserde::{DeBin, SerBin};

use naia_socket_shared::PacketReader;

use crate::property_mutate::PropertyMutator;

/// A Property of an Component/Message, that contains data
/// which must be tracked for updates
#[derive(Clone)]
pub struct Property<T: Clone + DeBin + SerBin + PartialEq> {
    inner: T,
    mutator: Option<PropertyMutator>,
    mutator_index: u8,
}

// should be shared
impl<T: Clone + DeBin + SerBin + PartialEq> Property<T> {
    /// Create a new Property
    pub fn new(value: T, mutator_index: u8) -> Property<T> {
        return Property::<T> {
            inner: value,
            mutator_index,
            mutator: None,
        };
    }

    /// Given a cursor into incoming packet data, initializes the Property with
    /// the synced value
    pub fn new_read(reader: &mut PacketReader, mutator_index: u8) -> Self {
        let length = reader.read_u8();

        let buffer = reader.get_buffer();
        let cursor = reader.get_cursor();

        let start: usize = cursor.position() as usize;
        let end: usize = start + (length as usize);

        let inner =
            DeBin::deserialize_bin(&buffer[start..end]).expect("error deserializing property");

        cursor.set_position(end as u64);

        return Property::<T> {
            inner,
            mutator_index,
            mutator: None,
        };
    }

    /// Gets a mutable reference to the value contained by the Property, queue
    /// to update
    pub fn get_mut(&mut self) -> &mut T {
        if let Some(mutator) = &mut self.mutator {
            mutator.mutate(self.mutator_index);
        }
        return &mut self.inner;
    }

    /// Set the Property's contained value
    pub fn set(&mut self, value: T) {
        if let Some(mutator) = &mut self.mutator {
            mutator.mutate(self.mutator_index);
        }
        self.inner = value;
    }

    /// Set an PropertyMutator to track changes to the Property
    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.mutator = Some(mutator.clone_new());
    }

    /// Gets a reference to the value contained by the Property
    pub fn get(&self) -> &T {
        return &self.inner;
    }

    /// Compare to another property
    pub fn equals(&self, other: &Property<T>) -> bool {
        return self.inner == other.inner;
    }

    /// Set value to the value of another Property
    pub fn mirror(&mut self, other: &Property<T>) {
        self.inner = other.inner.clone();
    }

    /// Writes contained value into outgoing byte stream
    pub fn write(&self, buffer: &mut Vec<u8>) {
        let encoded = &mut SerBin::serialize_bin(&self.inner);
        buffer.push(encoded.len() as u8);
        buffer.append(encoded);
    }
}
