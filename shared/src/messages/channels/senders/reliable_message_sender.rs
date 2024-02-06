
use naia_serde::BitWriter;
use naia_socket_shared::Instant;

use crate::{LocalEntityAndGlobalEntityConverterMut, LocalResponseId, messages::{
    channels::senders::{
        channel_sender::{ChannelSender, MessageChannelSender},
        indexed_message_writer::IndexedMessageWriter,
    },
    message_container::MessageContainer,
    message_kinds::MessageKinds,
}, ReliableSender, types::MessageIndex};
use crate::messages::channels::senders::request_sender::{LocalRequestId, RequestSender};
use crate::messages::request::GlobalRequestId;

// Sender
pub struct ReliableMessageSender {
    reliable_sender: ReliableSender<MessageContainer>,
    request_sender: RequestSender,
}

impl ReliableMessageSender {
    pub fn new(rtt_resend_factor: f32) -> Self {
        Self {
            reliable_sender: ReliableSender::new(rtt_resend_factor),
            request_sender: RequestSender::new(),
        }
    }
}

impl ChannelSender<MessageContainer> for ReliableMessageSender {
    fn send_message(&mut self, message: MessageContainer) {
        self.reliable_sender.send_message(message);
    }

    fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        self.reliable_sender.collect_messages(now, rtt_millis);
    }

    fn has_messages(&self) -> bool {
        self.reliable_sender.has_messages()
    }

    fn notify_message_delivered(&mut self, message_index: &MessageIndex) {
        self.reliable_sender.notify_message_delivered(message_index);
    }
}

impl MessageChannelSender for ReliableMessageSender {
    fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        IndexedMessageWriter::write_messages(
            message_kinds,
            &mut self.reliable_sender.outgoing_messages,
            converter,
            writer,
            has_written,
        )
    }

    fn send_outgoing_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        global_request_id: GlobalRequestId,
        request: MessageContainer
    ) {
        let processed_request = self.request_sender.process_outgoing_request(
            message_kinds,
            converter,
            global_request_id,
            request,
        );
        self.send_message(processed_request);
    }

    fn send_outgoing_response(&mut self, message_kinds: &MessageKinds, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut, local_response_id: LocalResponseId, response: MessageContainer) {
        let processed_response = self.request_sender.process_outgoing_response(
            message_kinds,
            converter,
            local_response_id,
            response,
        );
        self.send_message(processed_response);
    }

    fn process_incoming_response(&mut self, local_request_id: &LocalRequestId) -> Option<GlobalRequestId> {
        self.request_sender.process_incoming_response(local_request_id)
    }
}
