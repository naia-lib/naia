use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde};
use std::{collections::VecDeque, time::Duration};

use naia_socket_shared::Instant;

use crate::{
    protocol::protocolize::Protocolize, read_list_header, types::MessageId, write_list_header,
    ChannelIndex, Manifest, NetEntityHandleConverter, MTU_SIZE_BITS,
};

use super::channel_config::ReliableSettings;

pub trait MessageChannel<P: Protocolize, C: ChannelIndex> {
    fn send_message(&mut self, message: P);
    fn collect_outgoing_messages(&mut self, rtt_millis: &f32);
    fn collect_incoming_messages(&mut self, incoming_messages: &mut Vec<(C, P)>);
    fn notify_message_delivered(&mut self, message_id: &MessageId);
    fn has_outgoing_messages(&self) -> bool;
    fn write_messages(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>>;
    fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    );
}

pub struct OutgoingReliableChannel<P: Protocolize> {
    rtt_resend_factor: f32,
    oldest_waiting_message_id: MessageId,
    waiting_messages: VecDeque<Option<(MessageId, Option<Instant>, P)>>,
    ready_messages: VecDeque<(MessageId, P)>,
}

impl<P: Protocolize> OutgoingReliableChannel<P> {
    pub fn new(reliable_settings: &ReliableSettings) -> Self {
        Self {
            rtt_resend_factor: reliable_settings.rtt_resend_factor,
            oldest_waiting_message_id: 0,
            waiting_messages: VecDeque::new(),
            ready_messages: VecDeque::new(),
        }
    }

    pub fn send_message(&mut self, message: P) {
        self.waiting_messages
            .push_back(Some((self.oldest_waiting_message_id, None, message)));
        self.oldest_waiting_message_id = self.oldest_waiting_message_id.wrapping_add(1);
    }

    pub fn generate_messages(&mut self, rtt_millis: &f32) {
        let resend_duration = Duration::from_millis((self.rtt_resend_factor * rtt_millis) as u64);
        let now = Instant::now();

        for message_opt in self.waiting_messages.iter_mut() {
            if let Some((message_id, last_sent_opt, message)) = message_opt {
                let mut should_send = false;
                if let Some(last_sent) = last_sent_opt {
                    if last_sent.elapsed() >= resend_duration {
                        should_send = true;
                    }
                } else {
                    should_send = true;
                }
                if should_send {
                    self.ready_messages
                        .push_back((*message_id, message.clone()));
                    *last_sent_opt = Some(now.clone());
                }
            }
        }
    }

    pub fn notify_message_delivered(&mut self, message_id: &MessageId) {
        let mut index = 0;
        let mut found = false;

        loop {
            if index == self.waiting_messages.len() {
                break;
            }

            if let Some(Some((old_message_id, _, _))) = self.waiting_messages.get(index) {
                if *message_id == *old_message_id {
                    found = true;
                }
            }

            if found {
                // replace found message with nothing
                let container = self.waiting_messages.get_mut(index).unwrap();
                *container = None;

                // keep popping off Nones from the front of the Vec
                loop {
                    let mut pop = false;
                    if let Some(message_opt) = self.waiting_messages.front() {
                        if message_opt.is_none() {
                            pop = true;
                        }
                    }
                    if pop {
                        self.waiting_messages.pop_front();
                    } else {
                        break;
                    }
                }

                // stop loop
                break;
            }

            index += 1;
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.ready_messages.len() != 0;
    }

    pub fn write_messages(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>> {
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                write_list_header(writer, 0);
                return None;
            }

            let mut counter = BitCounter::new();

            //TODO: message_count is inaccurate here and may be different than final, does
            // this matter?
            write_list_header(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                write_list_header(writer, 0);
                return None;
            }

            // Find how many messages will fit into the packet
            let mut index = 0;
            loop {
                if index >= self.ready_messages.len() {
                    break;
                }

                let (message_id, message) = self.ready_messages.get(index).unwrap();
                self.write_message(converter, &mut counter, message_id, message);
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }

                index += 1;
            }
        }

        // Write header
        write_list_header(writer, message_count);

        // Messages
        {
            let mut message_ids = Vec::new();
            for _ in 0..message_count {
                // Pop and write message
                let (message_id, message) = self.ready_messages.pop_front().unwrap();
                self.write_message(converter, writer, &message_id, &message);

                message_ids.push(message_id);
            }
            return Some(message_ids);
        }
    }

    fn write_message<S: BitWrite>(
        &self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut S,
        message_id: &MessageId,
        message: &P,
    ) {
        // write message id
        message_id.ser(writer);

        // write message kind
        message.dyn_ref().kind().ser(writer);

        // write payload
        message.write(writer, converter);
    }

    pub fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    ) -> Vec<(MessageId, P)> {
        let message_count = read_list_header(reader);
        let mut output = Vec::new();
        for _x in 0..message_count {
            let id_w_msg = self.read_message(reader, manifest, converter);
            output.push(id_w_msg);
        }
        return output;
    }

    fn read_message(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    ) -> (MessageId, P) {
        // read message id
        let message_id: MessageId = MessageId::de(reader).unwrap();

        // read message kind
        let component_kind: P::Kind = P::Kind::de(reader).unwrap();

        // read payload
        let new_message = manifest.create_replica(component_kind, reader, converter);

        return (message_id, new_message);
    }
}
