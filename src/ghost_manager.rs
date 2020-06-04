
use log::{info};

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

    pub fn notify_packet_delivered(&self, packet_index: u16) {
        info!("yay ghost manager notified DELIVERED!");
    }

    pub fn notify_packet_dropped(&self, packet_index: u16) {
        info!("yay ghost manager notified DROPPED!");
    }

    pub fn process_data(&mut self, reader: &PacketReader, manifest: &Manifest<T>) {

    }
}