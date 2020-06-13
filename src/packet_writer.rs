use byteorder::{BigEndian, WriteBytesExt};
use crate::{ManagerType, StandardHeader, Event, EventTypeGetter, Manifest, EventType, EntityType};

pub const MTU_SIZE: usize = 508 - StandardHeader::bytes_number();

pub struct PacketWriter {
    event_working_bytes: Vec<u8>,
    event_count: u8,
    pub entity_working_bytes: Vec<u8>,
    pub entity_message_count: u8,
}

impl PacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be used to read information from.
    pub fn new() -> PacketWriter {
        PacketWriter {
            event_working_bytes: Vec::<u8>::new(),
            event_count: 0,
            entity_working_bytes: Vec::<u8>::new(),
            entity_message_count: 0,
        }
    }

    pub fn has_bytes(&self) -> bool {
        return self.event_count != 0 || self.entity_message_count != 0;
    }

    pub fn get_bytes(&mut self) -> Box<[u8]> {

        let mut out_bytes = Vec::<u8>::new();

        let mut wrote_manager_type = false;

        //Write manager "header" (manager type & entity count)
        if self.event_count != 0 {
            out_bytes.write_u8(ManagerType::Event as u8).unwrap(); // write manager type
            wrote_manager_type = true;
            out_bytes.write_u8(self.event_count).unwrap(); // write number of events in the following message
            out_bytes.append(&mut self.event_working_bytes); // write event payload
            self.event_count = 0;
        }

        //Write manager "header" (manager type & entity count)
        if self.entity_message_count != 0 {
            //info!("writing {} entity message, with {} bytes", self.entity_message_count, self.entity_working_bytes.len());
            if !wrote_manager_type {
                out_bytes.write_u8(ManagerType::Entity as u8).unwrap(); // write manager type
            }
            out_bytes.write_u8(self.entity_message_count).unwrap(); // write number of messages
            out_bytes.append(&mut self.entity_working_bytes); // write event payload

            self.entity_message_count = 0;
        }

        out_bytes.into_boxed_slice()
    }

    pub fn bytes_number(&self) -> usize {
        return self.event_working_bytes.len() + self.entity_working_bytes.len();
    }

    pub fn write_event<T: EventType, U: EntityType>(&mut self, manifest: &Manifest<T, U>, event: &Box<dyn Event<T>>) -> bool {
        //Write event payload
        let mut event_payload_bytes = Vec::<u8>::new();
        event.as_ref().write(&mut event_payload_bytes);
        if event_payload_bytes.len() > 255 {
            error!("cannot encode an event with more than 255 bytes, need to implement this");
        }

        //Write event "header" (event id & payload length)
        let mut event_total_bytes = Vec::<u8>::new();

        let type_id = EventTypeGetter::get_type_id(event.as_ref());
        let gaia_id = manifest.get_event_gaia_id(&type_id); // get gaia id
        event_total_bytes.write_u16::<BigEndian>(gaia_id).unwrap();// write gaia id
        event_total_bytes.write_u8(event_payload_bytes.len() as u8).unwrap(); // write payload length
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