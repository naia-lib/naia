use std::io::Cursor;

use nanoserde::{DeBin, SerBin};

use byteorder::ReadBytesExt;

use super::property::Property;

/// A Property that can read/write itself from/into incoming/outgoing packets
pub trait PropertyIo<T> {
    /// Writes contained value into outgoing byte stream
    fn write(&self, buffer: &mut Vec<u8>);
    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    fn read(&mut self, cursor: &mut Cursor<&[u8]>);
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
}
