use std::mem;

use naia_serde::{BitReader, SerdeErr};

use crate::{LocalEntityAndGlobalEntityConverter, MessageContainer, MessageKind, messages::{
    channels::{receivers::{
        channel_receiver::{ChannelReceiver, MessageChannelReceiver},
        indexed_message_reader::IndexedMessageReader,
    }, senders::request_sender::LocalRequestResponseId},
    message_kinds::MessageKinds,
}, sequence_greater_than, types::MessageIndex, world::remote::entity_waitlist::{EntityWaitlist, WaitlistStore}};

pub struct SequencedUnreliableReceiver {
    newest_received_message_index: Option<MessageIndex>,
    incoming_messages: Vec<MessageContainer>,
    waitlist_store: WaitlistStore<(MessageIndex, MessageContainer)>,
}

impl SequencedUnreliableReceiver {
    pub fn new() -> Self {
        Self {
            newest_received_message_index: None,
            incoming_messages: Vec::new(),
            waitlist_store: WaitlistStore::new(),
        }
    }

    pub fn buffer_message(
        &mut self,
        entity_waitlist: &mut EntityWaitlist,
        message_index: MessageIndex,
        message: MessageContainer,
    ) {
        if let Some(entity_set) = message.relations_waiting() {
            entity_waitlist.queue(
                &entity_set,
                &mut self.waitlist_store,
                (message_index, message),
            );
            return;
        }

        self.arrange_message(message_index, message);
    }

    pub fn arrange_message(&mut self, message_index: MessageIndex, message: MessageContainer) {
        if let Some(most_recent_id) = self.newest_received_message_index {
            if sequence_greater_than(message_index, most_recent_id) {
                self.incoming_messages.push(message);
                self.newest_received_message_index = Some(message_index);
            }
        } else {
            self.incoming_messages.push(message);
            self.newest_received_message_index = Some(message_index);
        }
    }
}

impl ChannelReceiver<MessageContainer> for SequencedUnreliableReceiver {
    fn receive_messages(
        &mut self,
        _message_kinds: &MessageKinds,
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Vec<MessageContainer> {
        if let Some(list) = entity_waitlist.collect_ready_items(&mut self.waitlist_store) {
            for (message_index, mut message) in list {
                message.relations_complete(converter);
                self.arrange_message(message_index, message);
            }
        }

        Vec::from(mem::take(&mut self.incoming_messages))
    }
}

impl MessageChannelReceiver for SequencedUnreliableReceiver {
    /// Read messages and add them to the buffer, discard messages that are older
    /// than the most recent received message
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let id_w_msgs = IndexedMessageReader::read_messages(message_kinds, converter, reader)?;
        for (id, message) in id_w_msgs {
            self.buffer_message(entity_waitlist, id, message);
        }
        Ok(())
    }

    fn receive_requests(&mut self) -> Vec<(MessageKind, LocalRequestResponseId, MessageContainer)> {
        panic!("SequencedUnreliable channels do not support requests");
    }
}
