use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

/// Contains an underlying byte payload, and provides a Cursor into that payload
pub struct PacketReader<'s> {
    buffer: &'s [u8],
    cursor: Cursor<&'s [u8]>,
}

impl<'s> PacketReader<'s> {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be
    /// used to read information from.
    pub fn new(buffer: &'s [u8]) -> PacketReader<'s> {
        PacketReader {
            buffer,
            cursor: Cursor::new(buffer),
        }
    }

    /// Returns whether there are still more bytes to be read from the payload
    pub fn has_more(&self) -> bool {
        return (self.cursor.position() as usize) < self.buffer.len();
    }

    /// Read a single byte from the payload
    pub fn read_u8(&mut self) -> u8 {
        return self.cursor.read_u8().unwrap();
    }

    /// Read a u16 from the payload
    pub fn read_u16(&mut self) -> u16 {
        return self.cursor.read_u16::<BigEndian>().unwrap();
    }

    /// Get a reference to the Cursor
    pub fn get_cursor(&mut self) -> &mut Cursor<&'s [u8]> {
        return &mut self.cursor;
    }

    /// Get a reference to the underlying payload byte buffer
    pub fn get_buffer(&self) -> &'s [u8] {
        return &self.buffer;
    }
}
