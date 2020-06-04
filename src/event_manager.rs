
use log::{info};

use crate::{ManifestType, NetEvent, NetEventClone, NetBase};
use std::{
    collections::VecDeque,
    rc::Rc};

pub struct EventManager<T: ManifestType> {
    queued_events: VecDeque<Box<dyn NetEvent<T>>>
}

impl<T: ManifestType> EventManager<T> {
    pub fn new() -> Self {
        EventManager {
            queued_events: VecDeque::new()
        }
    }

    pub fn notify_packet_delivered(&self, packet_index: u16) {
        info!("yay event manager notified DELIVERED!");
    }

    pub fn notify_packet_dropped(&self, packet_index: u16) {
        info!("yay event manager notified DROPPED!");
    }

    pub fn has_queued_events(&self) -> bool {
        return self.queued_events.len() != 0;
    }

    pub fn pop_event(&mut self) -> Option<Box<dyn NetEvent<T>>> {
        return self.queued_events.pop_front();
    }

    pub fn queue_event(&mut self, event: &impl NetEvent<T>) {
        let clone = NetEventClone::clone_box(event);
        self.queued_events.push_back(clone);
    }
}