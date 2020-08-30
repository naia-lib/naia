use std::io::Cursor;

use nanoserde::{DeBin, SerBin};

use byteorder::ReadBytesExt;

use super::property::Property;

use crate::sequence_buffer::sequence_greater_than;

/// A Property that can read/write itself from/into incoming/outgoing packets
pub trait PropertyIo<T> {
    /// Writes contained value into outgoing byte stream
    fn write(&self, buffer: &mut Vec<u8>);
    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    fn read(&mut self, cursor: &mut Cursor<&[u8]>);
    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value, but only if data is newer than the last data received
    fn read_seq(&mut self, cursor: &mut Cursor<&[u8]>, packet_index: u16);
}

impl<T: Clone + DeBin + SerBin> PropertyIo<T> for Property<T> {
    fn write(&self, buffer: &mut Vec<u8>) {
        let encoded = &mut SerBin::serialize_bin(&self.inner);
        buffer.push(encoded.len() as u8);
        buffer.append(encoded);
    }

    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        let length = cursor.read_u8().unwrap();
        let mut buffer = Vec::with_capacity(length as usize);
        for _ in 0..length {
            buffer.push(cursor.read_u8().unwrap());
        }
        self.inner = DeBin::deserialize_bin(&buffer[..]).unwrap();
    }

    fn read_seq(&mut self, cursor: &mut Cursor<&[u8]>, packet_index: u16) {
        let length = cursor.read_u8().unwrap();
        let mut buffer = Vec::with_capacity(length as usize);
        for _ in 0..length {
            buffer.push(cursor.read_u8().unwrap());
        }
        if sequence_greater_than(packet_index, self.last_recv_index) {
            self.last_recv_index = packet_index;
            self.inner = DeBin::deserialize_bin(&buffer[..]).unwrap();
        }
    }
}
