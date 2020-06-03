use std::io::{Cursor};
use byteorder::{BigEndian, ReadBytesExt};
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

    // currently returns a gaia id & payload
    pub fn read_event(&mut self) -> Option<(u16, Box<[u8]>)> {
        let manager_type: ManagerType = self.cursor.read_u8().unwrap().into();
        match manager_type {
            ManagerType::Event => {
                let event_count: u8 = self.cursor.read_u8().unwrap().into();
                for _x in 0..event_count {
                    let gaia_id: u16 = self.cursor.read_u16::<BigEndian>().unwrap().into();
                    let payload_length: u8 = self.cursor.read_u8().unwrap().into();
                    let payload_start_position: usize = self.cursor.position() as usize;
                    let payload_end_position: usize = payload_start_position + (payload_length as usize);

                    let boxed_payload = self.buffer[payload_start_position..payload_end_position]
                        .to_vec()
                        .into_boxed_slice();

                    return Some((gaia_id, boxed_payload));
                }
            }
            _ => {}
        }

        return None;
    }
}