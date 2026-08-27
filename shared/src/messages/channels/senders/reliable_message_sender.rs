use naia_serde::BitWriter;
use naia_socket_shared::Instant;

use crate::constants::FRAGMENTATION_LIMIT_BITS;
use crate::messages::channels::senders::message_fragmenter::MessageFragmenter;
use crate::messages::channels::senders::request_sender::{LocalRequestId, RequestSender};
use crate::messages::request::GlobalRequestId;
use crate::{
    messages::{
        channels::senders::{
            channel_sender::{ChannelSender, MessageChannelSender},
            indexed_message_writer::IndexedMessageWriter,
        },
        message_container::MessageContainer,
        message_kinds::MessageKinds,
    },
    types::MessageIndex,
    LocalEntityAndGlobalEntityConverterMut, LocalResponseId, ReliableSender,
};

// Sender
pub struct ReliableMessageSender {
    reliable_sender: ReliableSender<MessageContainer>,
    request_sender: RequestSender,
    message_fragmenter: MessageFragmenter,
}

impl ReliableMessageSender {
    pub fn new(rtt_resend_factor: f32, max_queue_depth: Option<usize>) -> Self {
        Self {
            reliable_sender: ReliableSender::new(rtt_resend_factor, max_queue_depth),
            request_sender: RequestSender::new(),
            message_fragmenter: MessageFragmenter::new(),
        }
    }
}

impl ReliableMessageSender {
    /// Queue `message` directly if it fits in one packet, or fragment it first.
    /// Returns `false` if the queue-depth cap left no room, in which case NOTHING
    /// was enqueued — a fragmented message is all-or-nothing.
    ///
    /// The all-or-nothing part is load-bearing. Pushing fragments one at a time and
    /// ignoring the per-fragment result can enqueue a *prefix* of a fragmented
    /// message: the receiver's `FragmentReceiver` then holds an entry that can never
    /// complete, so the logical message is lost even though the queue later drains,
    /// and on an ordered channel every later message behind it is stuck too. Testing
    /// capacity for the whole fragment set up front makes a full queue a clean,
    /// reportable refusal instead of silent corruption.
    fn send_or_fragment(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        message: MessageContainer,
    ) -> bool {
        if message.bit_length(message_kinds, converter) > FRAGMENTATION_LIMIT_BITS {
            let fragments =
                self.message_fragmenter
                    .fragment_message(message_kinds, converter, message);
            if !self.reliable_sender.has_capacity_for(fragments.len()) {
                return false;
            }
            for fragment in fragments {
                self.reliable_sender.send_message(fragment);
            }
            true
        } else {
            self.reliable_sender.send_message(message)
        }
    }
}

impl ChannelSender<MessageContainer> for ReliableMessageSender {
    fn send_message(&mut self, message: MessageContainer) -> bool {
        self.reliable_sender.send_message(message)
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
        let written = IndexedMessageWriter::write_messages(
            message_kinds,
            &mut self.reliable_sender.outgoing_messages,
            converter,
            writer,
            has_written,
        );
        // Stamp `last_sent` only for the messages that actually made it into the
        // packet, so truncated leftovers stay due and are re-collected next tick.
        if let Some(indices) = &written {
            self.reliable_sender.mark_written(indices);
        }
        written
    }

    fn send_outgoing_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        global_request_id: GlobalRequestId,
        request: MessageContainer,
    ) -> bool {
        let processed = self.request_sender.process_outgoing_request(
            message_kinds,
            converter,
            global_request_id,
            request,
        );
        self.send_or_fragment(message_kinds, converter, processed)
    }

    fn send_outgoing_response(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        local_response_id: LocalResponseId,
        response: MessageContainer,
    ) -> bool {
        let processed = self.request_sender.process_outgoing_response(
            message_kinds,
            converter,
            local_response_id,
            response,
        );
        self.send_or_fragment(message_kinds, converter, processed)
    }

    fn process_incoming_response(
        &mut self,
        local_request_id: &LocalRequestId,
    ) -> Option<GlobalRequestId> {
        self.request_sender
            .process_incoming_response(local_request_id)
    }
}
