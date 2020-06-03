use byteorder::{BigEndian, WriteBytesExt};
use crate::{ManagerType, NetEvent, Manifest, ManifestType};

pub struct PacketWriter {
}

impl PacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be used to read information from.
    pub fn new() -> PacketWriter {
        PacketWriter {

        }
    }

    pub fn write<T: ManifestType>(&mut self, manifest: &Manifest<T>, event: &impl NetEvent<T>) -> Box<[u8]> {
        //Write event payload
        let mut event_payload_bytes = Vec::<u8>::new();
        event.write(&mut event_payload_bytes);
        if event_payload_bytes.len() > 255 {
            error!("cannot encode an event with more than 255 bytes, need to implement this");
        }

        //Write event "header" (event id & payload length)
        let mut event_total_bytes = Vec::<u8>::new();

        let gaia_id = manifest.get_gaia_id(event); // get gaia id
        event_total_bytes.write_u16::<BigEndian>(gaia_id).unwrap();// write gaia id
        event_total_bytes.write_u8(event_payload_bytes.len() as u8).unwrap(); // write payload length
        event_total_bytes.append(&mut event_payload_bytes); // write payload

        //Write manager "header" (manager type & entity count)
        let mut out_bytes = Vec::<u8>::new();
        out_bytes.write_u8(ManagerType::Event as u8).unwrap(); // write manager type
        out_bytes.write_u8(1).unwrap(); // write number of events in the following message
        out_bytes.append(&mut event_total_bytes); // write event payload

        out_bytes.into_boxed_slice()
    }
}