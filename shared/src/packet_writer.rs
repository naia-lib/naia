use byteorder::{BigEndian, WriteBytesExt};

use crate::{
    entities::entity_type::EntityType,
    events::{event::Event, event_manager::EventManager, event_type::EventType},
    manager_type::ManagerType,
    manifest::Manifest,
    standard_header::StandardHeader,
};

/// The maximum of bytes that can be used for the payload of a given packet. (See #38 of http://ithare.com/64-network-dos-and-donts-for-game-engines-part-v-udp/)
pub const MTU_SIZE: usize = 508 - StandardHeader::bytes_number();

/// Handles writing of Event & Entity data into an outgoing packet
pub struct PacketWriter {
    event_working_bytes: Vec<u8>,
    event_count: u8,
    /// bytes representing outgoing Entity messages / updates
    pub entity_working_bytes: Vec<u8>,
    /// number of Entity messages to be written
    pub entity_message_count: u8,
    /// bytes representing outgoing Ping messages
    pub ping_working_bytes: Vec<u8>,
}

impl PacketWriter {
    /// Construct a new instance of PacketWriter
    pub fn new() -> PacketWriter {
        PacketWriter {
            event_working_bytes: Vec::<u8>::new(),
            event_count: 0,
            entity_working_bytes: Vec::<u8>::new(),
            entity_message_count: 0,
            ping_working_bytes: Vec::<u8>::new(),
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.event_count != 0 || self.entity_message_count != 0;
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn get_bytes(&mut self) -> Box<[u8]> {
        let mut out_bytes = Vec::<u8>::new();

        if self.ping_working_bytes.len() != 0 {
            out_bytes.write_u8(ManagerType::Ping as u8).unwrap(); // write manager type
            out_bytes.append(&mut self.ping_working_bytes); // write ping data
        }

        if self.event_count != 0 {
            out_bytes.write_u8(ManagerType::Event as u8).unwrap(); // write manager type
            out_bytes.write_u8(self.event_count).unwrap(); // write number of events in the following message
            out_bytes.append(&mut self.event_working_bytes); // write event payload
            self.event_count = 0;
        }

        if self.entity_message_count != 0 {
            out_bytes.write_u8(ManagerType::Entity as u8).unwrap(); // write manager type
            out_bytes.write_u8(self.entity_message_count).unwrap(); // write number of messages
            out_bytes.append(&mut self.entity_working_bytes); // write event payload
            self.entity_message_count = 0;
        }

        out_bytes.into_boxed_slice()
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.ping_working_bytes.len()
            + self.event_working_bytes.len()
            + self.entity_working_bytes.len();
    }

    /// Writes an Event into the Writer's internal buffer, which will eventually
    /// be put into the outgoing packet.
    /// Returns whether or not the event was able to be written
    pub fn write_event<T: EventType, U: EntityType>(
        &mut self,
        manifest: &Manifest<T, U>,
        event: &Box<dyn Event<T>>,
    ) -> bool {
        let mut event_total_bytes = EventManager::write_data(manifest, event);
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
