use std::io::{Cursor};
use byteorder::{ReadBytesExt};
use crate::{ManagerType};

pub struct PacketReader<'s> {
    buffer: &'s [u8],
    cursor: Cursor<&'s [u8]>,
}

impl<'s> PacketReader<'s> {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be used to read information from.
    pub fn new(buffer: &'s [u8]) -> PacketReader<'s> {
        PacketReader {
            buffer,
            cursor: Cursor::new(buffer),
        }
    }

    pub fn has_more(&self) -> bool {
        return (self.cursor.position() as usize) < self.buffer.len();
    }

    // currently returns a gaia id & payload
    pub fn read_manager_type(&mut self) -> ManagerType {
        return self.cursor.read_u8().unwrap().into();
    }

    pub fn get_cursor(&mut self) -> &mut Cursor<&'s [u8]> {
        return &mut self.cursor;
    }

    pub fn get_buffer(&self) -> &'s [u8] {
        return &self.buffer;
    }
}