use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{
    wrapping_diff, StateType, Event, EventPacketWriter, EventType, ManagerType,
    Manifest, MTU_SIZE, PawnKey
};

use crate::dual_command_receiver::DualCommandReceiver;

const MAX_PAST_COMMANDS: u8 = 3;

/// Handles writing of Event & State data into an outgoing packet
pub struct ClientPacketWriter {
    command_working_bytes: Vec<u8>,
    command_count: u8,
    event_writer: EventPacketWriter,
}

impl ClientPacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be
    /// used to read information from.
    pub fn new() -> ClientPacketWriter {
        ClientPacketWriter {
            command_working_bytes: Vec::<u8>::new(),
            command_count: 0,
            event_writer: EventPacketWriter::new(),
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.command_count != 0 || self.event_writer.has_bytes();
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn get_bytes(&mut self) -> Box<[u8]> {
        let mut out_bytes = Vec::<u8>::new();

        //Write manager "header" (manager type & state count)
        if self.command_count != 0 {
            out_bytes.write_u8(ManagerType::Command as u8).unwrap(); // write manager type
            out_bytes.write_u8(self.command_count).unwrap(); // write number of events in the following message
            out_bytes.append(&mut self.command_working_bytes); // write event payload
            self.command_count = 0;
        }

        self.event_writer.get_bytes(&mut out_bytes);

        out_bytes.into_boxed_slice()
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.command_working_bytes.len() + self.event_writer.bytes_number();
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_command<T: EventType, U: StateType>(
        &mut self,
        host_tick: u16,
        manifest: &Manifest<T, U>,
        command_receiver: &DualCommandReceiver<T>,
        pawn_key: &PawnKey,
        command: &Box<dyn State<T>>,
    ) -> bool {
        //Write command payload
        let mut command_payload_bytes = Vec::<u8>::new();

        command.as_ref().event_write(&mut command_payload_bytes);

        // write past commands
        let past_commands_number = command_receiver
            .command_history_count(&pawn_key)
            .min(MAX_PAST_COMMANDS);
        let mut past_command_index: u8 = 0;

        if let Some(mut iter) = command_receiver.command_history_iter(&pawn_key, true) {
            while past_command_index < past_commands_number {
                if let Some((past_tick, past_command)) = iter.next() {
                    // get tick diff between commands
                    let diff_i8: i16 = wrapping_diff(past_tick, host_tick);
                    if diff_i8 > 0 && diff_i8 <= 255 {
                        // write the tick diff
                        command_payload_bytes.write_u8(diff_i8 as u8).unwrap();
                        // write the command payload
                        past_command.event_write(&mut command_payload_bytes);

                        past_command_index += 1;
                    }
                } else {
                    break;
                }
            }
        }

        //Write command "header"
        let mut command_total_bytes = Vec::<u8>::new();


        match pawn_key {
            PawnKey::State(_) => {
                command_total_bytes
                    .write_u8(0)
                    .unwrap(); // write pawn type
            }
            PawnKey::Entity(_) => {
                command_total_bytes
                    .write_u8(255)
                    .unwrap(); // write pawn type
            }
        }
        command_total_bytes
            .write_u16::<BigEndian>(pawn_key.to_u16())
            .unwrap(); // write pawn key

        let type_id = command.as_ref().event_get_type_id();
        let naia_id = manifest.get_event_naia_id(&type_id); // get naia id
        command_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
        command_total_bytes.write_u8(past_command_index).unwrap(); // write past command number
        command_total_bytes.append(&mut command_payload_bytes); // write payload

        let mut hypothetical_next_payload_size = self.bytes_number() + command_total_bytes.len();
        if self.command_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            self.command_count += 1;
            self.command_working_bytes.append(&mut command_total_bytes);
            return true;
        } else {
            return false;
        }
    }

    /// Writes an Event into the Writer's internal buffer, which will eventually
    /// be put into the outgoing packet
    pub fn write_event<T: EventType, U: StateType>(
        &mut self,
        manifest: &Manifest<T, U>,
        event: &Box<dyn State<T>>,
    ) -> bool {
        return self.event_writer.write_event(manifest, event);
    }
}
