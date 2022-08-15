use std::{collections::VecDeque, time::Duration};

use log::info;

use naia_shared::{
    message_list_header, sequence_greater_than, sequence_less_than,
    serde::{BitCounter, BitWrite, BitWriter, Serde, UnsignedVariableInteger},
    wrapping_diff, ChannelWriter, Instant, Protocolize, ShortMessageId, Tick, TickBufferSettings,
    MESSAGE_HISTORY_SIZE, MTU_SIZE_BITS,
};

pub struct ChannelTickBufferSender<P: Protocolize> {
    sending_messages: OutgoingMessages<P>,
    next_send_messages: VecDeque<(Tick, Vec<(ShortMessageId, P)>)>,
    resend_interval: Duration,
    resend_interval_millis: u32,
    last_sent: Instant,
}

impl<P: Protocolize> ChannelTickBufferSender<P> {
    pub fn new(tick_duration: &Duration, settings: &TickBufferSettings) -> Self {
        let resend_interval = Duration::from_millis(
            ((settings.tick_resend_factor as u128) * tick_duration.as_millis()) as u64,
        );

        Self {
            sending_messages: OutgoingMessages::new(),
            next_send_messages: VecDeque::new(),
            resend_interval,
            resend_interval_millis: resend_interval.as_millis() as u32,
            last_sent: Instant::now(),
        }
    }

    pub fn collect_outgoing_messages(
        &mut self,
        client_sending_tick: &Tick,
        server_receivable_tick: &Tick,
    ) {
        if self.last_sent.elapsed() >= self.resend_interval {
            // Remove messages that would never be able to reach the Server
            self.sending_messages
                .pop_back_until_excluding(server_receivable_tick);

            self.last_sent = Instant::now();

            // Loop through outstanding messages and add them to the outgoing list
            for (message_tick, message_map) in self.sending_messages.iter() {
                if sequence_greater_than(*message_tick, *client_sending_tick) {
                    //info!("found message that is more recent than client sending tick! (how?)");
                    break;
                }
                let messages = message_map.collect_messages();
                self.next_send_messages.push_back((*message_tick, messages));
            }

            // if self.next_send_messages.len() > 0 {
            //     info!("next_send_messages.len() = {} messages",
            // self.next_send_messages.len()); }
        }
    }

    pub fn send_message(&mut self, host_tick: &Tick, message: P) {
        self.sending_messages.push(*host_tick, message);

        self.last_sent = Instant::now();
        self.last_sent.subtract_millis(self.resend_interval_millis);
    }

    pub fn has_outgoing_messages(&self) -> bool {
        !self.next_send_messages.is_empty()
    }

    // Tick Buffer Message Writing

    pub fn write_messages(
        &mut self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut BitWriter,
        host_tick: &Tick,
    ) -> Option<Vec<(Tick, ShortMessageId)>> {
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = bit_writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                message_list_header::write(bit_writer, 0);
                return None;
            }

            let mut counter = BitCounter::new();

            //TODO: message_count is inaccurate here and may be different than final, does
            // this matter?
            message_list_header::write(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                message_list_header::write(bit_writer, 0);
                return None;
            }

            // Find how many messages will fit into the packet
            let mut last_written_tick = *host_tick;
            let mut index = 0;
            loop {
                if index >= self.next_send_messages.len() {
                    break;
                }

                let (message_tick, messages) = self.next_send_messages.get(index).unwrap();
                self.write_message(
                    channel_writer,
                    &mut counter,
                    &last_written_tick,
                    message_tick,
                    messages,
                );
                last_written_tick = *message_tick;
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }

                index += 1;
            }
        }

        // Write header
        message_list_header::write(bit_writer, message_count);

        // Messages
        {
            let mut last_written_tick = *host_tick;
            let mut output = Vec::new();
            for _ in 0..message_count {
                // Pop message
                let (message_tick, messages) = self.next_send_messages.pop_front().unwrap();

                // Write message
                let message_ids = self.write_message(
                    channel_writer,
                    bit_writer,
                    &last_written_tick,
                    &message_tick,
                    &messages,
                );
                last_written_tick = message_tick;
                for message_id in message_ids {
                    output.push((message_tick, message_id));
                }
            }
            Some(output)
        }
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    fn write_message(
        &self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut dyn BitWrite,
        last_written_tick: &Tick,
        message_tick: &Tick,
        messages: &Vec<(ShortMessageId, P)>,
    ) -> Vec<ShortMessageId> {
        let mut message_ids = Vec::new();

        // write message tick diff
        // this is reversed (diff is always negative, but it's encoded as positive)
        // because packet tick is always larger than past ticks
        let message_tick_diff = wrapping_diff(*message_tick, *last_written_tick);
        let message_tick_diff_encoded = UnsignedVariableInteger::<3>::new(message_tick_diff);
        message_tick_diff_encoded.ser(bit_writer);

        // write number of messages
        let message_count = UnsignedVariableInteger::<3>::new(messages.len() as u64);
        message_count.ser(bit_writer);

        let mut last_id_written: ShortMessageId = 0;
        for (message_id, message) in messages {
            // write message id diff
            let id_diff = UnsignedVariableInteger::<2>::new(*message_id - last_id_written);
            id_diff.ser(bit_writer);

            // write payload
            channel_writer.write(bit_writer, message);

            // record id for output
            message_ids.push(*message_id);
            last_id_written = *message_id;
        }

        message_ids
    }

    pub fn notify_message_delivered(&mut self, tick: &Tick, message_id: &ShortMessageId) {
        self.sending_messages.remove_message(tick, message_id);
    }
}

// MessageMap
struct MessageMap<P: Protocolize> {
    list: Vec<Option<P>>,
}

impl<P: Protocolize> MessageMap<P> {
    pub fn new() -> Self {
        MessageMap { list: Vec::new() }
    }

    pub fn insert(&mut self, message: P) {
        self.list.push(Some(message));
    }

    pub fn collect_messages(&self) -> Vec<(ShortMessageId, P)> {
        let mut output = Vec::new();
        for (index, message_opt) in self.list.iter().enumerate() {
            if let Some(message) = message_opt {
                output.push((index as u8, message.clone()));
            }
        }
        output
    }

    pub fn remove(&mut self, message_id: &ShortMessageId) {
        if let Some(container) = self.list.get_mut(*message_id as usize) {
            *container = None;
        }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }
}

// OutgoingMessages

struct OutgoingMessages<P: Protocolize> {
    // front big, back small
    // front recent, back past
    buffer: VecDeque<(Tick, MessageMap<P>)>,
}

impl<P: Protocolize> OutgoingMessages<P> {
    pub fn new() -> Self {
        OutgoingMessages {
            buffer: VecDeque::new(),
        }
    }

    // should only push increasing ticks of messages
    pub fn push(&mut self, message_tick: Tick, message_protocol: P) {
        if let Some((front_tick, msg_map)) = self.buffer.front_mut() {
            if message_tick == *front_tick {
                // been here before, cool
                msg_map.insert(message_protocol);
                return;
            }

            if sequence_less_than(message_tick, *front_tick) {
                panic!("this method should always receive increasing or equal ticks!")
            }
        } else {
            // nothing is in here
        }

        let mut msg_map = MessageMap::new();
        msg_map.insert(message_protocol);
        self.buffer.push_front((message_tick, msg_map));

        // a good time to prune down this list
        while self.buffer.len() > MESSAGE_HISTORY_SIZE.into() {
            self.buffer.pop_back();
            info!("pruning outgoing_messages buffer cause it got too big");
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

    pub fn iter(&self) -> impl Iterator<Item = &(Tick, MessageMap<P>)> {
        self.buffer.iter()
    }

    pub fn remove_message(&mut self, tick: &Tick, message_id: &ShortMessageId) {
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
                    message_map.remove(message_id);
                    //info!("removed delivered message! tick: {}, msg_id: {}", tick, msg_id);
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
}
