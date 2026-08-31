use std::collections::VecDeque;

use naia_serde::BitWriter;
use naia_socket_shared::Instant;

use crate::messages::channels::senders::request_sender::LocalRequestId;
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
    LocalEntityAndGlobalEntityConverterMut, LocalResponseId,
};

pub struct SequencedUnreliableSender {
    /// Buffer of the next messages to send along with their MessageKind
    outgoing_messages: VecDeque<(MessageIndex, MessageContainer)>,
    /// Next message id to use (not yet used in the buffer)
    next_send_message_index: MessageIndex,
}

impl SequencedUnreliableSender {
    pub fn new() -> Self {
        Self {
            outgoing_messages: VecDeque::new(),
            next_send_message_index: 0,
        }
    }
}

// Drop oldest entry when the queue grows beyond this bound. For unreliable
// channels, evicting the oldest message is semantically correct.
const MAX_QUEUE_DEPTH: usize = 1024;

impl ChannelSender<MessageContainer> for SequencedUnreliableSender {
    fn send_message(&mut self, message: MessageContainer) -> bool {
        if self.outgoing_messages.len() >= MAX_QUEUE_DEPTH {
            self.outgoing_messages.pop_front();
        }
        self.outgoing_messages
            .push_back((self.next_send_message_index, message));
        self.next_send_message_index = self.next_send_message_index.wrapping_add(1);
        true
    }

    fn collect_messages(&mut self, _: &Instant, _: &f32) {
        // not necessary for an unreliable channel
    }

    fn has_messages(&self) -> bool {
        !self.outgoing_messages.is_empty()
    }

    fn notify_message_delivered(&mut self, _: &MessageIndex) {
        // not necessary for an unreliable channel
    }
}

impl MessageChannelSender for SequencedUnreliableSender {
    /// Write messages from the buffer into the channel
    /// Include a wrapped message id for sequencing purposes
    fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        IndexedMessageWriter::write_messages(
            message_kinds,
            &mut self.outgoing_messages,
            converter,
            writer,
            has_written,
        )
    }

    fn send_outgoing_request(
        &mut self,
        _: &MessageKinds,
        _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        _: GlobalRequestId,
        _: MessageContainer,
    ) -> bool {
        panic!("SequencedUnreliable channel does not support requests");
    }

    fn send_outgoing_response(
        &mut self,
        _: &MessageKinds,
        _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        _: LocalResponseId,
        _: MessageContainer,
    ) -> bool {
        panic!("SequencedUnreliable channel does not support requests");
    }

    fn process_incoming_response(&mut self, _: &LocalRequestId) -> Option<GlobalRequestId> {
        panic!("SequencedUnreliable channel does not support requests");
    }
}

#[cfg(test)]
mod sequenced_unreliable_sender_tests {
    //! Queue and sequencing behaviour for [`SequencedUnreliableSender`].
    //!
    //! This sender differs from the unordered one in exactly one respect: it
    //! stamps each message with a monotonically increasing `MessageIndex` so the
    //! receiver can discard anything older than what it has already seen. The
    //! tests below pin that stamp -- including its wrap, which is the case a
    //! reader cannot verify by inspection because `MessageIndex` is a `u16` and
    //! the counter is never reset for the life of the connection.

    use naia_serde::{BitReader, Serde};

    use super::*;
    use crate::{
        messages::{
            channels::receivers::indexed_message_reader::IndexedMessageReader,
            fragment::{FragmentId, FragmentIndex, FragmentedMessage},
        },
        FakeEntityConverter, MessageKinds,
    };

    fn kinds() -> MessageKinds {
        let mut kinds = MessageKinds::new();
        kinds.add_message::<FragmentedMessage>();
        kinds
    }

    fn tagged(tag: u8, len: usize) -> MessageContainer {
        let mut bytes = vec![0u8; len];
        bytes[0] = tag;
        MessageContainer::new(Box::new(FragmentedMessage::new(
            FragmentId::zero(),
            FragmentIndex::from_u32(0),
            bytes.into_boxed_slice(),
        )))
    }

    fn tag_of(message: MessageContainer) -> u8 {
        message
            .to_boxed_any()
            .downcast::<FragmentedMessage>()
            .expect("the round trip should yield the message that went in")
            .to_payload()[0]
    }

    /// Drains one packet's worth and decodes `(index, tag)` pairs back out.
    fn drain(sender: &mut SequencedUnreliableSender) -> Vec<(MessageIndex, u8)> {
        let kinds = kinds();
        let mut writer = BitWriter::new();
        let mut has_written = false;
        sender.write_messages(
            &kinds,
            &mut FakeEntityConverter,
            &mut writer,
            &mut has_written,
        );

        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let mut out = Vec::new();
        let mut last_index = None;
        // As with the unordered sender, the terminating `false` bit is the
        // caller's to write, so the stream ends at a zero bit or a read error.
        while matches!(bool::de(&mut reader), Ok(true)) {
            let index = IndexedMessageReader::read_message_index(&mut reader, &last_index)
                .expect("the index the sender just wrote should read back");
            last_index = Some(index);
            let message = kinds
                .read(&mut reader, &FakeEntityConverter)
                .expect("a message the sender just wrote should read back");
            out.push((index, tag_of(message)));
        }
        out
    }

    // -- the queue ----------------------------------------------------------

    #[test]
    fn a_fresh_sender_has_nothing_to_send() {
        let sender = SequencedUnreliableSender::new();
        assert!(!sender.has_messages());
    }

    #[test]
    fn a_sent_message_is_queued_and_accepted() {
        let mut sender = SequencedUnreliableSender::new();
        assert!(sender.send_message(tagged(1, 4)));
        assert!(sender.has_messages());
    }

    #[test]
    fn the_queue_is_bounded_and_evicts_the_oldest_message() {
        let mut sender = SequencedUnreliableSender::new();
        for _ in 0..MAX_QUEUE_DEPTH {
            sender.send_message(tagged(1, 4));
        }
        sender.send_message(tagged(2, 4));

        let mut all = Vec::new();
        loop {
            let batch = drain(&mut sender);
            if batch.is_empty() {
                break;
            }
            all.extend(batch);
        }

        assert_eq!(
            all.len(),
            MAX_QUEUE_DEPTH,
            "the queue must never hold more than MAX_QUEUE_DEPTH messages"
        );
        assert_eq!(
            all[0].0, 1,
            "index 0 was the message evicted, so the survivors start at 1 -- \
             eviction drops the OLDEST, and the index stamp proves which one"
        );
        assert_eq!(
            all.last().unwrap().1,
            2,
            "the newest message must be the one kept"
        );
    }

    // -- sequencing ---------------------------------------------------------

    #[test]
    fn each_message_is_stamped_with_the_next_index() {
        let mut sender = SequencedUnreliableSender::new();
        for tag in 0..5u8 {
            sender.send_message(tagged(tag, 4));
        }

        assert_eq!(
            drain(&mut sender),
            vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)],
            "indices should be assigned at send time, in order, starting at zero"
        );
    }

    #[test]
    fn the_index_keeps_climbing_across_packets() {
        // The counter belongs to the connection, not the packet: a message sent
        // after a drain must not restart at zero, or the receiver would discard
        // it as stale.
        let mut sender = SequencedUnreliableSender::new();
        sender.send_message(tagged(0, 4));
        assert_eq!(drain(&mut sender), vec![(0, 0)]);

        sender.send_message(tagged(1, 4));
        assert_eq!(
            drain(&mut sender),
            vec![(1, 1)],
            "the second packet's message should carry index 1, not 0"
        );
    }

    #[test]
    fn the_index_wraps_rather_than_overflowing() {
        // MessageIndex is a u16 and is never reset, so a long-lived connection
        // WILL reach 65535. `wrapping_add` is what keeps that from panicking in
        // a debug build; this drives the counter all the way round to prove it.
        let mut sender = SequencedUnreliableSender::new();
        for _ in 0..=(MessageIndex::MAX as u32) {
            sender.send_message(tagged(0, 4));
            // Drain each time so the bounded queue never evicts, which would
            // make the count of sends and the count of indices diverge.
            drain(&mut sender);
        }

        sender.send_message(tagged(7, 4));
        assert_eq!(
            drain(&mut sender),
            vec![(0, 7)],
            "the index after u16::MAX must wrap back to 0 rather than overflow"
        );
    }

    // -- the parts of the trait an unreliable channel does not use ----------

    #[test]
    fn collecting_and_notifying_are_no_ops() {
        let mut sender = SequencedUnreliableSender::new();
        sender.send_message(tagged(1, 4));

        sender.collect_messages(&Instant::now(), &200.0);
        sender.notify_message_delivered(&7);

        assert_eq!(
            drain(&mut sender),
            vec![(0, 1)],
            "neither call should have disturbed the queue or the index counter"
        );
    }

    #[test]
    #[should_panic(expected = "does not support requests")]
    fn sending_a_request_is_a_programming_error() {
        let mut sender = SequencedUnreliableSender::new();
        sender.send_outgoing_request(
            &kinds(),
            &mut FakeEntityConverter,
            GlobalRequestId::new(0),
            tagged(1, 4),
        );
    }

    #[test]
    #[should_panic(expected = "does not support requests")]
    fn sending_a_response_is_a_programming_error() {
        let mut sender = SequencedUnreliableSender::new();
        sender.send_outgoing_response(
            &kinds(),
            &mut FakeEntityConverter,
            LocalRequestId::from(0).receive_from_remote(),
            tagged(1, 4),
        );
    }

    #[test]
    #[should_panic(expected = "does not support requests")]
    fn processing_a_response_is_a_programming_error() {
        let mut sender = SequencedUnreliableSender::new();
        sender.process_incoming_response(&LocalRequestId::from(0));
    }
}
