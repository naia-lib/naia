use std::collections::VecDeque;

use naia_serde::{BitWrite, BitWriter, Serde};
use naia_socket_shared::Instant;

use crate::messages::channels::senders::request_sender::LocalRequestId;
use crate::messages::request::GlobalRequestId;
use crate::{
    messages::{
        channels::senders::channel_sender::{ChannelSender, MessageChannelSender},
        message_container::MessageContainer,
        message_kinds::MessageKinds,
    },
    types::MessageIndex,
    LocalEntityAndGlobalEntityConverterMut, LocalResponseId,
};

pub struct UnorderedUnreliableSender {
    outgoing_messages: VecDeque<MessageContainer>,
}

impl UnorderedUnreliableSender {
    pub fn new() -> Self {
        Self {
            outgoing_messages: VecDeque::new(),
        }
    }

    fn write_message(
        &self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut dyn BitWrite,
        message: &MessageContainer,
    ) {
        message.write(message_kinds, writer, converter);
    }

    fn warn_overflow(&self, message: &MessageContainer, bits_needed: u32, bits_free: u32) {
        let message_name = message.name();
        panic!(
            "Packet Write Error: Blocking overflow detected! Message of type `{message_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommended to slim down this Message, or send this message over a Reliable channel so it can be Fragmented)"
        )
    }
}

// Drop oldest entry when the queue grows beyond this bound. For unreliable
// channels, evicting the oldest message is semantically correct.
const MAX_QUEUE_DEPTH: usize = 1024;

impl ChannelSender<MessageContainer> for UnorderedUnreliableSender {
    fn send_message(&mut self, message: MessageContainer) -> bool {
        if self.outgoing_messages.len() >= MAX_QUEUE_DEPTH {
            self.outgoing_messages.pop_front();
        }
        self.outgoing_messages.push_back(message);
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

impl MessageChannelSender for UnorderedUnreliableSender {
    fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        loop {
            if self.outgoing_messages.is_empty() {
                break;
            }

            let message = self.outgoing_messages.front().unwrap();

            // Check that we can write the next message
            let mut counter = writer.counter();
            // write MessageContinue bit
            true.ser(&mut counter);
            // write data
            self.write_message(message_kinds, converter, &mut counter, message);
            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of message being too big
                if !*has_written {
                    self.warn_overflow(message, counter.bits_needed(), writer.bits_free());
                }

                break;
            }

            *has_written = true;

            // write MessageContinue bit
            true.ser(writer);
            // write data
            self.write_message(message_kinds, converter, writer, message);

            // pop message we've written
            self.outgoing_messages.pop_front();
        }
        None
    }

    fn send_outgoing_request(
        &mut self,
        _: &MessageKinds,
        _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        _: GlobalRequestId,
        _: MessageContainer,
    ) -> bool {
        panic!("UnorderedUnreliable channel does not support requests");
    }

    fn process_incoming_response(&mut self, _: &LocalRequestId) -> Option<GlobalRequestId> {
        panic!("UnorderedUnreliable channel does not support requests");
    }

    fn send_outgoing_response(
        &mut self,
        _: &MessageKinds,
        _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        _: LocalResponseId,
        _: MessageContainer,
    ) -> bool {
        panic!("UnorderedUnreliable channel does not support requests");
    }
}

#[cfg(test)]
mod unordered_unreliable_sender_tests {
    //! Queue and packing behaviour for [`UnorderedUnreliableSender`].
    //!
    //! The two things worth pinning here are the ones a reader would have to
    //! take on trust: the queue is bounded and evicts from the FRONT (dropping
    //! the oldest is the semantically correct choice for an unreliable channel,
    //! but the opposite is just as easy to write), and a message that does not
    //! fit in the remaining packet space is left in the queue rather than
    //! silently dropped or half-written.

    use naia_serde::BitReader;

    use super::*;
    use crate::{
        messages::fragment::{FragmentId, FragmentIndex, FragmentedMessage},
        FakeEntityConverter, MessageContainer, MessageKinds,
    };

    fn kinds() -> MessageKinds {
        let mut kinds = MessageKinds::new();
        kinds.add_message::<FragmentedMessage>();
        kinds
    }

    /// A message tagged with `tag` in its payload, so it can be told apart from
    /// its neighbours after a round trip. `len` controls how much packet space
    /// it needs.
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
        let fragment = message
            .to_boxed_any()
            .downcast::<FragmentedMessage>()
            .expect("the round trip should yield the message that went in");
        fragment.to_payload()[0]
    }

    /// Drains `sender` into a packet and decodes the tags back out, which is the
    /// only honest way to ask what it actually wrote.
    fn drain(sender: &mut UnorderedUnreliableSender) -> Vec<u8> {
        let kinds = kinds();
        let mut writer = BitWriter::new();
        let mut has_written = false;
        let indices = sender.write_messages(
            &kinds,
            &mut FakeEntityConverter,
            &mut writer,
            &mut has_written,
        );
        assert!(
            indices.is_none(),
            "an unordered channel carries no message indices to notify on"
        );

        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let mut tags = Vec::new();
        // `write_messages` writes a `true` continue bit before each message but
        // never a terminating `false` -- that is the caller's job. So the stream
        // ends either at the zero-fill of the last byte (a `false` bit) or, when
        // the packet was filled exactly, at a read error.
        while matches!(bool::de(&mut reader), Ok(true)) {
            let message = kinds
                .read(&mut reader, &FakeEntityConverter)
                .expect("a message the sender just wrote should read back");
            tags.push(tag_of(message));
        }
        tags
    }

    // -- the queue ----------------------------------------------------------

    #[test]
    fn a_fresh_sender_has_nothing_to_send() {
        let sender = UnorderedUnreliableSender::new();
        assert!(!sender.has_messages(), "a new sender's queue is empty");
    }

    #[test]
    fn a_sent_message_is_queued_and_accepted() {
        let mut sender = UnorderedUnreliableSender::new();
        assert!(
            sender.send_message(tagged(1, 4)),
            "an unreliable channel always accepts a message"
        );
        assert!(sender.has_messages());
    }

    #[test]
    fn messages_are_written_in_the_order_they_were_sent() {
        let mut sender = UnorderedUnreliableSender::new();
        for tag in 1..=5u8 {
            sender.send_message(tagged(tag, 4));
        }

        assert_eq!(
            drain(&mut sender),
            vec![1, 2, 3, 4, 5],
            "the queue is FIFO: the first message sent is the first written"
        );
        assert!(
            !sender.has_messages(),
            "every written message should have been popped"
        );
    }

    #[test]
    fn the_queue_is_bounded_and_evicts_the_oldest_message() {
        // MAX_QUEUE_DEPTH + 1 messages are sent; the first must be the one
        // dropped. Evicting the NEWEST would be just as easy to write and would
        // pass any test that only counted the survivors, so the tags matter.
        let mut sender = UnorderedUnreliableSender::new();
        for _ in 0..MAX_QUEUE_DEPTH {
            sender.send_message(tagged(1, 4));
        }
        sender.send_message(tagged(2, 4));

        let mut queued = 0;
        let mut first_tag = None;
        let mut last_tag = None;
        loop {
            let tags = drain(&mut sender);
            if tags.is_empty() {
                break;
            }
            if first_tag.is_none() {
                first_tag = Some(tags[0]);
            }
            last_tag = Some(*tags.last().unwrap());
            queued += tags.len();
        }

        assert_eq!(
            queued, MAX_QUEUE_DEPTH,
            "the queue must never hold more than MAX_QUEUE_DEPTH messages"
        );
        assert_eq!(
            first_tag,
            Some(1),
            "the survivors should start with the oldest message that was kept"
        );
        assert_eq!(
            last_tag,
            Some(2),
            "the newest message must be kept, and the OLDEST dropped -- if \
             eviction popped the back instead, tag 2 would be the one missing"
        );
    }

    #[test]
    fn the_queue_stays_at_capacity_under_sustained_sending() {
        let mut sender = UnorderedUnreliableSender::new();
        for _ in 0..(MAX_QUEUE_DEPTH * 2) {
            sender.send_message(tagged(1, 4));
        }

        let mut queued = 0;
        loop {
            let drained = drain(&mut sender).len();
            if drained == 0 {
                break;
            }
            queued += drained;
        }
        assert_eq!(
            queued, MAX_QUEUE_DEPTH,
            "the bound holds however long the sender is starved of packets"
        );
    }

    // -- packing ------------------------------------------------------------

    #[test]
    fn a_message_that_does_not_fit_is_left_in_the_queue() {
        // One message big enough to fill most of a packet, then a second of the
        // same size. The first is written, the second cannot fit, and must
        // survive for the next packet rather than being dropped or truncated.
        let mut sender = UnorderedUnreliableSender::new();
        sender.send_message(tagged(1, 400));
        sender.send_message(tagged(2, 400));

        assert_eq!(
            drain(&mut sender),
            vec![1],
            "only the message that fits should be written"
        );
        assert!(
            sender.has_messages(),
            "the message that did not fit must still be queued"
        );
        assert_eq!(
            drain(&mut sender),
            vec![2],
            "and it should go out in the next packet"
        );
    }

    #[test]
    fn writing_into_a_full_packet_writes_nothing_and_keeps_the_queue() {
        // `has_written` is already true, so the sender must break quietly rather
        // than take the overflow-warning path.
        let kinds = kinds();
        let mut sender = UnorderedUnreliableSender::new();
        sender.send_message(tagged(1, 400));

        let mut writer = BitWriter::new();
        // Fill the packet first.
        let mut has_written = false;
        let mut filler = UnorderedUnreliableSender::new();
        for _ in 0..8 {
            filler.send_message(tagged(9, 400));
        }
        filler.write_messages(
            &kinds,
            &mut FakeEntityConverter,
            &mut writer,
            &mut has_written,
        );
        assert!(has_written, "the filler should have written something");

        sender.write_messages(
            &kinds,
            &mut FakeEntityConverter,
            &mut writer,
            &mut has_written,
        );
        assert!(
            sender.has_messages(),
            "a sender that cannot fit its message into an already-full packet \
             must retain it"
        );
    }

    #[test]
    #[should_panic(expected = "Blocking overflow detected")]
    fn a_message_too_big_for_an_empty_packet_panics() {
        // Nothing has been written yet, so the message can never be sent on this
        // channel at any packet boundary. Silently dropping it would strand the
        // caller; the panic names the message type so the protocol author can
        // fix it.
        let mut sender = UnorderedUnreliableSender::new();
        sender.send_message(tagged(1, 4096));
        drain(&mut sender);
    }

    // -- the parts of the trait an unreliable channel does not use ----------

    #[test]
    fn collecting_and_notifying_are_no_ops() {
        let mut sender = UnorderedUnreliableSender::new();
        sender.send_message(tagged(1, 4));

        sender.collect_messages(&Instant::now(), &200.0);
        sender.notify_message_delivered(&7);

        assert_eq!(
            drain(&mut sender),
            vec![1],
            "neither call should have disturbed the queue: an unreliable channel \
             has no resend state to advance"
        );
    }

    #[test]
    #[should_panic(expected = "does not support requests")]
    fn sending_a_request_is_a_programming_error() {
        let mut sender = UnorderedUnreliableSender::new();
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
        let mut sender = UnorderedUnreliableSender::new();
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
        let mut sender = UnorderedUnreliableSender::new();
        sender.process_incoming_response(&LocalRequestId::from(0));
    }
}
