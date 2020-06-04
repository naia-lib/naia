use byteorder::{BigEndian, WriteBytesExt};
use crate::{ManagerType, NetEvent, NetBase, NetBaseClone, NetBaseType, Manifest, ManifestType};

pub struct PacketWriter {
    working_bytes: Vec<u8>,
    event_count: u8,
}

impl PacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be used to read information from.
    pub fn new() -> PacketWriter {
        PacketWriter {
            working_bytes: Vec::<u8>::new(),
            event_count: 0,
        }
    }

    pub fn has_bytes(&self) -> bool {
        return self.event_count != 0;
    }

    pub fn get_bytes(&mut self) -> Box<[u8]> {

        let mut out_bytes = Vec::<u8>::new();

        //Write manager "header" (manager type & entity count)
        out_bytes.write_u8(ManagerType::Event as u8).unwrap(); // write manager type
        out_bytes.write_u8(self.event_count).unwrap(); // write number of events in the following message
        out_bytes.append(&mut self.working_bytes); // write event payload

        out_bytes.into_boxed_slice()
    }

    pub fn write_event<T: ManifestType>(&mut self, manifest: &Manifest<T>, event: &Box<dyn NetEvent<T>>) {
        //Write event payload
        let mut event_payload_bytes = Vec::<u8>::new();
        event.as_ref().write(&mut event_payload_bytes);
        if event_payload_bytes.len() > 255 {
            error!("cannot encode an event with more than 255 bytes, need to implement this");
        }

        //Write event "header" (event id & payload length)
        let mut event_total_bytes = Vec::<u8>::new();

        let boxed_type = NetBaseClone::clone_box(event.as_ref());
        let type_id = NetBaseType::get_type_id(&boxed_type);
        let gaia_id = manifest.get_gaia_id(&type_id); // get gaia id
        event_total_bytes.write_u16::<BigEndian>(gaia_id).unwrap();// write gaia id
        event_total_bytes.write_u8(event_payload_bytes.len() as u8).unwrap(); // write payload length
        event_total_bytes.append(&mut event_payload_bytes); // write payload

        self.event_count += 1;

        self.working_bytes.append(&mut event_total_bytes);
    }
}