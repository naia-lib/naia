use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
    vec::Vec,
};

use crate::{
    state::{state::State, state_type::StateType, state::EventClone},
    manifest::Manifest,
    PacketReader,
};

/// Handles incoming/outgoing events, tracks the delivery status of Events so
/// that guaranteed Events can be re-transmitted to the remote host
#[derive(Debug)]
pub struct EventManager<T: StateType> {
    queued_outgoing_events: VecDeque<Rc<Box<dyn State<T>>>>,
    queued_incoming_events: VecDeque<T>,
    sent_events: HashMap<u16, Vec<Rc<Box<dyn State<T>>>>>,
}

impl<T: StateType> EventManager<T> {
    /// Creates a new EventManager
    pub fn new() -> Self {
        EventManager {
            queued_outgoing_events: VecDeque::new(),
            queued_incoming_events: VecDeque::new(),
            sent_events: HashMap::new(),
        }
    }

    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Events in that packet.
    pub fn notify_packet_delivered(&mut self, packet_index: u16) {
        self.sent_events.remove(&packet_index);
    }

    /// Occurs when a packet has been notified as having been dropped. Queues up
    /// any guaranteed Events that were lost in the packet for retransmission.
    pub fn notify_packet_dropped(&mut self, packet_index: u16) {
        if let Some(dropped_events_list) = self.sent_events.get(&packet_index) {
            for dropped_event in dropped_events_list.into_iter() {
                self.queued_outgoing_events.push_back(dropped_event.clone());
            }

            self.sent_events.remove(&packet_index);
        }
    }

    /// Returns whether the Manager has queued Events that can be transmitted to
    /// the remote host
    pub fn has_outgoing_events(&self) -> bool {
        return self.queued_outgoing_events.len() != 0;
    }

    /// Gets the next queued Event to be transmitted
    pub fn pop_outgoing_event(&mut self, packet_index: u16) -> Option<Rc<Box<dyn State<T>>>> {
        match self.queued_outgoing_events.pop_front() {
            Some(event) => {
                //place in transmission record if this is a gauranteed event
                if State::event_is_guaranteed(event.as_ref().as_ref()) {
                    if !self.sent_events.contains_key(&packet_index) {
                        let sent_events_list: Vec<Rc<Box<dyn State<T>>>> = Vec::new();
                        self.sent_events.insert(packet_index, sent_events_list);
                    }

                    if let Some(sent_events_list) = self.sent_events.get_mut(&packet_index) {
                        sent_events_list.push(event.clone());
                    }
                }

                Some(event)
            }
            None => None,
        }
    }

    /// If  the last popped Event from the queue somehow wasn't able to be
    /// written into a packet, put the Event back into the front of the queue
    pub fn unpop_outgoing_event(&mut self, packet_index: u16, event: &Rc<Box<dyn State<T>>>) {
        let cloned_event = event.clone();

        if State::event_is_guaranteed(event.as_ref().as_ref()) {
            if let Some(sent_events_list) = self.sent_events.get_mut(&packet_index) {
                sent_events_list.pop();
                if sent_events_list.len() == 0 {
                    self.sent_events.remove(&packet_index);
                }
            }
        }

        self.queued_outgoing_events.push_front(cloned_event);
    }

    /// Queues an Event to be transmitted to the remote host
    pub fn queue_outgoing_event(&mut self, event: &impl State<T>) {
        let clone = Rc::new(EventClone::event_clone_box(event));
        self.queued_outgoing_events.push_back(clone);
    }

    /// Returns whether any Events have been received that must be handed to the
    /// application
    pub fn has_incoming_events(&self) -> bool {
        return self.queued_incoming_events.len() != 0;
    }

    /// Get the most recently received Event
    pub fn pop_incoming_event(&mut self) -> Option<T> {
        return self.queued_incoming_events.pop_front();
    }

    /// Given incoming packet data, read transmitted Events and store them to be
    /// returned to the application
    pub fn process_data(
        &mut self,
        reader: &mut PacketReader,
        manifest: &Manifest<T>)
    {
        let event_count = reader.read_u8();
        for _x in 0..event_count {
            let naia_id: u16 = reader.read_u16();

            if let Some(new_event) = manifest.create_event(naia_id, reader) {
                self.queued_incoming_events.push_back(new_event);
            } else {
                panic!("using current manifest, can't create a new event with a NaiaId of {}", naia_id);
            }
        }
    }
}
