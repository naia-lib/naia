
use crate::{EntityType, PacketReader, EntityManifest, NetEntity};
use std::{
    collections::VecDeque};

pub struct ServerEntityManager<T: EntityType> {
    unused_list: VecDeque<Box<dyn NetEntity<T>>>,
}

impl<T: EntityType> ServerEntityManager<T> {
    pub fn new() -> Self {
        ServerEntityManager {
            unused_list: VecDeque::new()
        }
    }

    pub fn notify_packet_delivered(&mut self, packet_index: u16) {
    }

    pub fn notify_packet_dropped(&mut self, packet_index: u16) {
    }

    pub fn process_data(&mut self, reader: &mut PacketReader, manifest: &EntityManifest<T>) {

    }
}