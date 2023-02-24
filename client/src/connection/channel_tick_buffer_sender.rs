use std::collections::VecDeque;

use log::warn;

use naia_shared::{
    sequence_greater_than, sequence_less_than, wrapping_diff, BitWrite, BitWriter, Message,
    MessageKinds, NetEntityHandleConverter, Serde, ShortMessageIndex, Tick, TickBufferSettings,
    UnsignedVariableInteger,
};

pub struct ChannelTickBufferSender {
    sending_messages: OutgoingMessages,
    outgoing_messages: VecDeque<(Tick, Vec<(ShortMessageIndex, Box<dyn Message>)>)>,
    last_sent: Tick,
    never_sent: bool,
}

impl ChannelTickBufferSender {
    pub fn new(settings: TickBufferSettings) -> Self {
        Self {
            sending_messages: OutgoingMessages::new(settings.message_capacity),
            outgoing_messages: VecDeque::new(),
            last_sent: 0,
            never_sent: true,
        }
    }

    pub fn collect_outgoing_messages(
        &mut self,
        client_sending_tick: &Tick,
        server_receivable_tick: &Tick,
    ) {
        if sequence_greater_than(*client_sending_tick, self.last_sent) || self.never_sent {
            // Remove messages that would never be able to reach the Server
            self.sending_messages
                .pop_back_until_excluding(server_receivable_tick);

            self.last_sent = *client_sending_tick;
            self.never_sent = true;

            // Loop through outstanding messages and add them to the outgoing list
            for (message_tick, message_map) in self.sending_messages.iter() {
                if sequence_greater_than(*message_tick, *client_sending_tick) {
                    warn!("Sending message that is more recent than client sending tick! This shouldn't be possible.");
                    break;
                }

                let messages = message_map.collect_messages();
                self.outgoing_messages.push_back((*message_tick, messages));
            }
        }
    }

    pub fn send_message(&mut self, host_tick: &Tick, message: Box<dyn Message>) {
        self.sending_messages.push(*host_tick, message);
    }

    pub fn has_messages(&self) -> bool {
        !self.outgoing_messages.is_empty()
    }

    // Tick Buffer Message Writing

    pub fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
        host_tick: &Tick,
        has_written: &mut bool,
    ) -> Option<Vec<(Tick, ShortMessageIndex)>> {
        let mut last_written_tick = *host_tick;
        let mut output = Vec::new();

        loop {
            if self.outgoing_messages.is_empty() {
                break;
            }

            let (message_tick, messages) = self.outgoing_messages.front().unwrap();

            // check that we can write the next message
            let mut counter = writer.counter();
            self.write_message(
                message_kinds,
                converter,
                &mut counter,
                &last_written_tick,
                message_tick,
                messages,
            );

            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of message being too big
                if !*has_written {
                    self.warn_overflow(messages, counter.bits_needed(), writer.bits_free());
                }

                break;
            }

            *has_written = true;

            // write MessageContinue bit
            true.ser(writer);

            // write data
            let message_indexs = self.write_message(
                message_kinds,
                converter,
                writer,
                &last_written_tick,
                &message_tick,
                &messages,
            );
            last_written_tick = *message_tick;
            for message_index in message_indexs {
                output.push((*message_tick, message_index));
            }

            // pop message we've written
            self.outgoing_messages.pop_front();
        }
        Some(output)
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    fn write_message(
        &self,
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut dyn BitWrite,
        last_written_tick: &Tick,
        message_tick: &Tick,
        messages: &Vec<(ShortMessageIndex, Box<dyn Message>)>,
    ) -> Vec<ShortMessageIndex> {
        let mut message_indexs = Vec::new();

        // write message tick diff
        // this is reversed (diff is always negative, but it's encoded as positive)
        // because packet tick is always larger than past ticks
        let message_tick_diff = wrapping_diff(*message_tick, *last_written_tick);
        let message_tick_diff_encoded = UnsignedVariableInteger::<3>::new(message_tick_diff);
        message_tick_diff_encoded.ser(writer);

        // write number of messages
        let message_count = UnsignedVariableInteger::<3>::new(messages.len() as u64);
        message_count.ser(writer);

        let mut last_id_written: ShortMessageIndex = 0;
        for (message_index, message) in messages {
            // write message id diff
            let id_diff = UnsignedVariableInteger::<2>::new(*message_index - last_id_written);
            id_diff.ser(writer);

            // write payload
            message.write(message_kinds, writer, converter);

            // record id for output
            message_indexs.push(*message_index);
            last_id_written = *message_index;
        }

        message_indexs
    }

    pub fn notify_message_delivered(&mut self, tick: &Tick, message_index: &ShortMessageIndex) {
        self.sending_messages.remove_message(tick, message_index);
    }

    fn warn_overflow(
        &self,
        messages: &Vec<(ShortMessageIndex, Box<dyn Message>)>,
        bits_needed: u16,
        bits_free: u16,
    ) {
        let mut message_names = "".to_string();
        let mut added = false;
        for (_id, message) in messages {
            if added {
                message_names.push(',');
            } else {
                added = true;
            }
            message_names.push_str(&message.name());
        }
        panic!(
            "Packet Write Error: Blocking overflow detected! Messages of type `{message_names}` requires {bits_needed} bits, but packet only has {bits_free} bits available! This condition should never be reached, as large Messages should be Fragmented in the Reliable channel"
        )
    }
}

// MessageMap
struct MessageMap {
    list: Vec<Option<Box<dyn Message>>>,
}

impl MessageMap {
    pub fn new() -> Self {
        MessageMap { list: Vec::new() }
    }

    pub fn insert(&mut self, message: Box<dyn Message>) {
        self.list.push(Some(message));
    }

    pub fn collect_messages(&self) -> Vec<(ShortMessageIndex, Box<dyn Message>)> {
        let mut output = Vec::new();
        for (index, message_opt) in self.list.iter().enumerate() {
            if let Some(message) = message_opt {
                output.push((index as u8, message.clone()));
            }
        }
        output
    }

    pub fn remove(&mut self, message_index: &ShortMessageIndex) {
        if let Some(container) = self.list.get_mut(*message_index as usize) {
            *container = None;
        }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }
}

// OutgoingMessages

struct OutgoingMessages {
    // front big, back small
    // front recent, back past
    buffer: VecDeque<(Tick, MessageMap)>,
    // this is the maximum length of the buffer
    capacity: usize,
}

impl OutgoingMessages {
    pub fn new(capacity: usize) -> Self {
        OutgoingMessages {
            buffer: VecDeque::new(),
            capacity,
        }
    }

    // should only push increasing ticks of messages
    pub fn push(&mut self, message_tick: Tick, message_protocol: Box<dyn Message>) {
        if let Some((front_tick, msg_map)) = self.buffer.front_mut() {
            if message_tick == *front_tick {
                // been here before, cool
                msg_map.insert(message_protocol);
                return;
            }

            if sequence_less_than(message_tick, *front_tick) {
                warn!("This method should always receive increasing or equal Ticks! \
                Received Tick: {message_tick} after receiving {front_tick}. \
                Possibly try ensuring that Client.send_message() is only called on this channel once per Tick?");
                return;
            }
        } else {
            // nothing is in here
        }

        let mut msg_map = MessageMap::new();
        msg_map.insert(message_protocol);
        self.buffer.push_front((message_tick, msg_map));

        // a good time to prune down this list
        while self.buffer.len() > self.capacity {
            self.buffer.pop_back();
        }
    }

    pub fn pop_back_until_excluding(&mut self, until_tick: &Tick) {
        loop {
            if let Some((old_tick, _)) = self.buffer.back() {
                if sequence_less_than(*until_tick, *old_tick) {
                    return;
                }
            } else {
                return;
            }

            self.buffer.pop_back();
        }
    }

    pub fn remove_message(&mut self, tick: &Tick, message_index: &ShortMessageIndex) {
        let mut index = self.buffer.len();

        if index == 0 {
            // empty condition
            return;
        }

        loop {
            index -= 1;

            let mut remove = false;

            if let Some((old_tick, message_map)) = self.buffer.get_mut(index) {
                if *old_tick == *tick {
                    // found it!
                    message_map.remove(message_index);
                    if message_map.len() == 0 {
                        remove = true;
                    }
                } else {
                    // if tick is less than old tick, no sense continuing, only going to get bigger
                    // as we go
                    if sequence_greater_than(*old_tick, *tick) {
                        return;
                    }
                }
            }

            if remove {
                self.buffer.remove(index);
            }

            if index == 0 {
                return;
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Tick, MessageMap)> {
        self.buffer.iter()
    }
}
