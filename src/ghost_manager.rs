
use crate::{ManifestType, PacketReader, Manifest, NetEvent};
use std::{
    collections::VecDeque};

pub struct GhostManager<T: ManifestType> {
    unused_list: VecDeque<Box<dyn NetEvent<T>>>
}

impl<T: ManifestType> GhostManager<T> {
    pub fn new() -> Self {
        GhostManager {
            unused_list: VecDeque::new()
        }
    }

    pub fn notify_packet_delivered(&mut self, packet_index: u16) {
    }

    pub fn notify_packet_dropped(&mut self, packet_index: u16) {
    }

    pub fn process_data(&mut self, reader: &PacketReader, manifest: &Manifest<T>) {

    }
}