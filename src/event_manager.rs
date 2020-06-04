
use log::{info};

use std::{
    collections::VecDeque,
    rc::Rc};
use std::io::{Cursor};
use byteorder::{BigEndian, ReadBytesExt};

use crate::{ManifestType, NetEvent, NetEventClone, NetBase, PacketReader, Manifest};

pub struct EventManager<T: ManifestType> {
    queued_outgoing_events: VecDeque<Box<dyn NetEvent<T>>>,
    queued_incoming_events: VecDeque<T>
}

impl<T: ManifestType> EventManager<T> {
    pub fn new() -> Self {
        EventManager {
            queued_outgoing_events: VecDeque::new(),
            queued_incoming_events: VecDeque::new(),
        }
    }

    pub fn notify_packet_delivered(&self, packet_index: u16) {
        info!("yay event manager notified DELIVERED!");
    }

    pub fn notify_packet_dropped(&self, packet_index: u16) {
        info!("yay event manager notified DROPPED!");
    }

    pub fn has_outgoing_events(&self) -> bool {
        return self.queued_outgoing_events.len() != 0;
    }

    pub fn pop_outgoing_event(&mut self) -> Option<Box<dyn NetEvent<T>>> {
        return self.queued_outgoing_events.pop_front();
    }

    pub fn queue_outgoing_event(&mut self, event: &impl NetEvent<T>) {
        let clone = NetEventClone::clone_box(event);
        self.queued_outgoing_events.push_back(clone);
    }

    pub fn has_incoming_events(&self) -> bool {
        return self.queued_incoming_events.len() != 0;
    }

    pub fn pop_incoming_event(&mut self) -> Option<T> {
        return self.queued_incoming_events.pop_front();
    }

    pub fn process_data(&mut self, reader: &mut PacketReader, manifest: &Manifest<T>) {
        let buffer = reader.get_buffer();
        let cursor = reader.get_cursor();

        let event_count: u8 = cursor.read_u8().unwrap().into();
        for _x in 0..event_count {
            let gaia_id: u16 = cursor.read_u16::<BigEndian>().unwrap().into();
            let payload_length: u8 = cursor.read_u8().unwrap().into();
            let payload_start_position: usize = cursor.position() as usize;
            let payload_end_position: usize = payload_start_position + (payload_length as usize);


            let event_payload = buffer[payload_start_position..payload_end_position]
                .to_vec()
                .into_boxed_slice();

            match manifest.create_type(gaia_id) {
                Some(mut new_entity) => {
                    if new_entity.is_event() {
                        new_entity.use_bytes(&event_payload);
                        self.queued_incoming_events.push_back(new_entity);
                    }
                }
                _ => {}
            }
        }
    }
}