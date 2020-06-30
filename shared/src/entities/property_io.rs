use std::io::Cursor;

use byteorder::ReadBytesExt;

use super::property::Property;

/// A Property that can read/write itself from/into incoming/outgoing packets
pub trait PropertyIo<T> {
    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    fn read(&mut self, cursor: &mut Cursor<&[u8]>);
    /// Writes contained value into outgoing byte stream
    fn write(&self, buffer: &mut Vec<u8>);
}

//u8
impl PropertyIo<u8> for Property<u8> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_u8().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.inner);
    }
}

//String
impl PropertyIo<String> for Property<String> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        let length = cursor.read_u8().unwrap();
        let buffer = &mut Vec::with_capacity(length as usize);
        for _ in 0..length {
            buffer.push(cursor.read_u8().unwrap());
        }
        self.inner = String::from_utf8_lossy(buffer).to_string();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.inner.len() as u8);
        let mut bytes = self.inner.as_bytes().to_vec();
        buffer.append(&mut bytes);
    }
}
