use byteorder::{BigEndian, ReadBytesExt};
use std::collections::VecDeque;

use naia_shared::{EntityType, EventType, Manifest, PacketReader};

/// Handles incoming/outgoing events, tracks the delivery status of Events so
/// that guaranteed Events can be re-transmitted to the remote host
#[derive(Debug)]
pub struct CommandReceiver<T: EventType> {
    queued_incoming_commands: VecDeque<T>,
}

impl<T: EventType> CommandReceiver<T> {
    /// Creates a new CommandReceiver
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: VecDeque::new(),
        }
    }

    /// Get the most recently received Command
    pub fn pop_incoming_command(&mut self) -> Option<T> {
        return self.queued_incoming_commands.pop_front();
    }

    /// Given incoming packet data, read transmitted Events and store them to be
    /// returned to the application
    pub fn process_data<U: EntityType>(
        &mut self,
        reader: &mut PacketReader,
        manifest: &Manifest<T, U>,
    ) {
        let buffer = reader.get_buffer();
        let cursor = reader.get_cursor();

        let event_count = cursor.read_u8().unwrap();
        for _x in 0..event_count {
            let naia_id: u16 = cursor.read_u16::<BigEndian>().unwrap().into();
            let payload_length: u8 = cursor.read_u8().unwrap().into();
            let payload_start_position: usize = cursor.position() as usize;
            let payload_end_position: usize = payload_start_position + (payload_length as usize);

            let command_payload = buffer[payload_start_position..payload_end_position]
                .to_vec()
                .into_boxed_slice();

            match manifest.create_event(naia_id, &command_payload) {
                Some(new_event) => {
                    self.queued_incoming_commands.push_back(new_event);
                }
                _ => {}
            }

            cursor.set_position(payload_end_position as u64);
        }
    }
}
