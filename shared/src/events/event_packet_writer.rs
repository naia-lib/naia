use byteorder::{BigEndian, WriteBytesExt};

use crate::{
    actors::actor_type::ActorType,
    events::{event::Event, event_type::EventType},
    manager_type::ManagerType,
    manifest::Manifest,
    standard_header::StandardHeader,
};

/// The maximum of bytes that can be used for the payload of a given packet. (See #38 of http://ithare.com/64-network-dos-and-donts-for-game-engines-part-v-udp/)
pub const MTU_SIZE: usize = 508 - StandardHeader::bytes_number();

/// Handles writing of Event & Actor data into an outgoing packet
pub struct EventPacketWriter {
    event_working_bytes: Vec<u8>,
    event_count: u8,
}

impl EventPacketWriter {
    /// Construct a new instance of `EventPacketWriter`, the given `buffer` will
    /// be used to read information from.
    pub fn new() -> EventPacketWriter {
        EventPacketWriter {
            event_working_bytes: Vec::<u8>::new(),
            event_count: 0,
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.event_count != 0;
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn get_bytes(&mut self, out_bytes: &mut Vec<u8>) {
        //Write manager "header" (manager type & actor count)
        if self.event_count != 0 {
            out_bytes.write_u8(ManagerType::Event as u8).unwrap(); // write manager type
            out_bytes.write_u8(self.event_count).unwrap(); // write number of events in the following message
            out_bytes.append(&mut self.event_working_bytes); // write event payload
            self.event_count = 0;
        }
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.event_working_bytes.len();
    }

    /// Writes an Event into the Writer's internal buffer, which will eventually
    /// be put into the outgoing packet
    pub fn write_event<T: EventType, U: ActorType>(
        &mut self,
        manifest: &Manifest<T, U>,
        event: &Box<dyn Event<T>>,
    ) -> bool {
        //Write event payload
        let mut event_payload_bytes = Vec::<u8>::new();
        event.as_ref().write(&mut event_payload_bytes);

        //Write event "header"
        let mut event_total_bytes = Vec::<u8>::new();

        let type_id = event.as_ref().get_type_id();
        let naia_id = manifest.get_event_naia_id(&type_id); // get naia id
        event_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
        event_total_bytes.append(&mut event_payload_bytes); // write payload

        let mut hypothetical_next_payload_size = self.bytes_number() + event_total_bytes.len();
        if self.event_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            self.event_count += 1;
            self.event_working_bytes.append(&mut event_total_bytes);
            return true;
        } else {
            return false;
        }
    }
}
