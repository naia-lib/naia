use log::{trace, warn};

use naia_serde::{BitReader, SerdeErr};
use naia_socket_shared::Instant;

use crate::world::local::local_world_manager::LocalWorldManager;
use crate::{
    messages::{
        channels::{
            receivers::{
                channel_receiver::{ChannelReceiver, MessageChannelReceiver, RequestsAndResponses},
                fragment_receiver::FragmentReceiver,
                indexed_message_reader::IndexedMessageReader,
                reliable_receiver::ReliableReceiver,
            },
            senders::request_sender::{LocalRequestId, LocalRequestOrResponseId},
        },
        message_kinds::MessageKinds,
    },
    types::MessageIndex,
    world::remote::remote_entity_waitlist::{RemoteEntityWaitlist, WaitlistStore},
    ConnectionRequestNonce, LocalEntityAndGlobalEntityConverter, LocalResponseId, MessageContainer,
    RequestOrResponse,
};

// Receiver Arranger Trait
pub trait ReceiverArranger: Send + Sync {
    fn process(
        &mut self,
        start_message_index: MessageIndex,
        end_message_index: MessageIndex,
        message: MessageContainer,
    ) -> Vec<MessageContainer>;
}

// Reliable Receiver
pub struct ReliableMessageReceiver<A: ReceiverArranger> {
    reliable_receiver: ReliableReceiver<MessageContainer>,
    incoming_messages: Vec<MessageContainer>,
    arranger: A,
    fragment_receiver: FragmentReceiver,
    waitlist_store: WaitlistStore<(MessageIndex, MessageIndex, MessageContainer)>,
    incoming_requests: Vec<(LocalResponseId, ConnectionRequestNonce, MessageContainer)>,
    incoming_responses: Vec<(LocalRequestId, ConnectionRequestNonce, MessageContainer)>,
}

impl<A: ReceiverArranger> ReliableMessageReceiver<A> {
    pub fn with_arranger(arranger: A) -> Self {
        Self::with_arranger_and_window(arranger, None)
    }

    /// `max_receive_window` is how many message indices ahead of the oldest
    /// outstanding one a peer may buffer, or `None` for no window. It is derived
    /// from the channel's `max_queue_depth`, which is the span an honest peer's
    /// sender is itself bounded by. See
    /// [`ReliableReceiver::buffer_message`](super::reliable_receiver::ReliableReceiver::buffer_message).
    pub fn with_arranger_and_window(arranger: A, max_receive_window: Option<u16>) -> Self {
        Self {
            reliable_receiver: ReliableReceiver::with_window(max_receive_window),
            incoming_messages: Vec::new(),
            arranger,
            fragment_receiver: FragmentReceiver::new(),
            waitlist_store: WaitlistStore::new(),
            incoming_requests: Vec::new(),
            incoming_responses: Vec::new(),
        }
    }

    fn push_message(
        &mut self,
        message_kinds: &MessageKinds,
        local_world_manager: &mut LocalWorldManager,
        message_index: MessageIndex,
        message: MessageContainer,
    ) {
        let Some((start_message_index, end_message_index, full_message)) = ({
            if message.is_fragment() {
                self.fragment_receiver.receive(
                    message_kinds,
                    &local_world_manager.entity_converter(),
                    message_index,
                    message,
                )
            } else {
                Some((message_index, message_index, message))
            }
        }) else {
            return;
        };

        if let Some(remote_entity_set) = full_message.relations_waiting() {
            warn!(
                "Queuing waiting message {:?}! Waiting on entities: {:?}",
                full_message.name(),
                remote_entity_set
            );
            local_world_manager.entity_waitlist_queue(
                &remote_entity_set,
                &mut self.waitlist_store,
                (start_message_index, end_message_index, full_message),
            );
            return;
        }
        trace!("Received message {:?}!", full_message.name());

        let incoming_messages =
            self.arranger
                .process(start_message_index, end_message_index, full_message);
        for message in incoming_messages {
            self.receive_message(
                message_kinds,
                &local_world_manager.entity_converter(),
                message,
            );
        }
    }

    pub fn buffer_message(
        &mut self,
        message_kinds: &MessageKinds,
        local_world_manager: &mut LocalWorldManager,
        message_index: MessageIndex,
        message: MessageContainer,
    ) {
        self.reliable_receiver
            .buffer_message(message_index, message);
        let received_messages = self.reliable_receiver.receive_messages();
        for (received_message_id, received_message) in received_messages {
            self.push_message(
                message_kinds,
                local_world_manager,
                received_message_id,
                received_message,
            );
        }
    }

    fn receive_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        message_container: MessageContainer,
    ) {
        // look at message, see if it's a request or response
        if message_container.is_request_or_response() {
            // it is! cast it
            let request_or_response_container = message_container
                .to_boxed_any()
                .downcast::<RequestOrResponse>()
                .unwrap();
            let (local_id, wire_nonce, request_bytes) =
                request_or_response_container.to_id_and_bytes();
            let mut reader = BitReader::new(&request_bytes);
            let request_or_response = match message_kinds.read(&mut reader, converter) {
                Ok(msg) => msg,
                Err(e) => {
                    // Malformed request/response from remote — discard and continue rather
                    // than crashing the connection. This can arise from a buggy peer or
                    // deliberate fuzzing; the connection stays up.
                    warn!("Discarding malformed request/response message ({e}); dropping packet.");
                    return;
                }
            };

            // add it to incoming requests or responses, keeping the wire
            // nonce: the response path echoes it, and the request path
            // resolves on (local id, nonce).
            match local_id {
                LocalRequestOrResponseId::Request(local_request_id) => {
                    let request = request_or_response;
                    let local_response_id = local_request_id.receive_from_remote();
                    self.incoming_requests
                        .push((local_response_id, wire_nonce, request));
                }
                LocalRequestOrResponseId::Response(local_response_id) => {
                    let response = request_or_response;
                    let local_request_id = local_response_id.receive_from_remote();
                    self.incoming_responses
                        .push((local_request_id, wire_nonce, response));
                }
            }
        } else {
            // it's not a request, just add it to incoming messages
            self.incoming_messages.push(message_container);
        }
    }
}

impl<A: ReceiverArranger> ChannelReceiver<MessageContainer> for ReliableMessageReceiver<A> {
    fn receive_messages(
        &mut self,
        message_kinds: &MessageKinds,
        now: &Instant,
        entity_waitlist: &mut RemoteEntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Vec<MessageContainer> {
        if let Some(list) = entity_waitlist.collect_ready_items(now, &mut self.waitlist_store) {
            for (start_message_index, end_message_index, mut full_message) in list {
                if !full_message.relations_complete(converter) {
                    warn!(
                        "Dropping waitlisted reliable message: an awaited entity relation is still unresolvable (stale redirect or missing mapping)."
                    );
                    continue;
                }
                let incoming_messages =
                    self.arranger
                        .process(start_message_index, end_message_index, full_message);
                for message in incoming_messages {
                    self.receive_message(message_kinds, converter, message);
                }
            }
        }

        std::mem::take(&mut self.incoming_messages)
    }
}

impl<A: ReceiverArranger> MessageChannelReceiver for ReliableMessageReceiver<A> {
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        local_world_manager: &mut LocalWorldManager,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let id_w_msgs = IndexedMessageReader::read_messages(
            message_kinds,
            &local_world_manager.entity_converter(),
            reader,
        )?;
        for (id, message) in id_w_msgs {
            self.buffer_message(message_kinds, local_world_manager, id, message);
        }
        Ok(())
    }

    fn receive_requests_and_responses(&mut self) -> RequestsAndResponses {
        (
            std::mem::take(&mut self.incoming_requests),
            std::mem::take(&mut self.incoming_responses),
        )
    }
}

#[cfg(test)]
mod retransmit_tests {
    use super::*;
    use crate::{
        messages::channels::{
            receivers::{
                ordered_reliable_receiver::OrderedReliableReceiver,
                unordered_reliable_receiver::UnorderedReliableReceiver,
            },
            senders::message_fragmenter::MessageFragmenter,
        },
        world::test_support::TestGwm,
        ComponentKinds, FakeEntityConverter, HostType, Protocol,
    };
    use naia_derive::MessageInternal;

    #[derive(MessageInternal)]
    struct Body {
        value: String,
    }

    fn retransmit_behind_gap<A: ReceiverArranger>(
        mut receiver: ReliableMessageReceiver<A>,
        ordered: bool,
    ) {
        let mut protocol = Protocol::builder();
        protocol.add_message::<Body>();
        let protocol = protocol.build();
        let kinds = &protocol.message_kinds;
        let components = ComponentKinds::new();
        let gwm = TestGwm::new(&components);
        let mut world = LocalWorldManager::new(&None, HostType::Server, 0, &gwm);
        let fragments = MessageFragmenter::new().fragment_message(
            kinds,
            &mut FakeEntityConverter,
            MessageContainer::new(Box::new(Body {
                value: "x".repeat(1000),
            })),
        );
        assert_eq!(fragments.len(), 3);
        // Keep index 0 missing, so the deduplication record retains 1..3.
        // buffer_message drains the inner reliable receiver on every arrival.
        for _ in 0..2 {
            for (offset, fragment) in fragments.iter().enumerate() {
                receiver.buffer_message(kinds, &mut world, offset as u16 + 1, fragment.clone());
            }
        }
        assert_eq!(receiver.incoming_messages.len(), usize::from(!ordered));
        receiver.buffer_message(
            kinds,
            &mut world,
            0,
            MessageContainer::new(Box::new(Body {
                value: "gap".into(),
            })),
        );
        assert_eq!(receiver.incoming_messages.len(), 2);
        let bodies: Vec<_> = receiver
            .incoming_messages
            .into_iter()
            .map(|message| message.to_boxed_any().downcast::<Body>().unwrap().value)
            .collect();
        assert_eq!(
            bodies
                .iter()
                .filter(|value| **value == "x".repeat(1000))
                .count(),
            1
        );
        assert_eq!(
            bodies
                .iter()
                .filter(|value| value.as_str() == "gap")
                .count(),
            1
        );
    }

    #[test]
    fn ordered_fragment_retransmits_behind_gap_deliver_once() {
        retransmit_behind_gap(OrderedReliableReceiver::new(), true);
    }

    #[test]
    fn unordered_fragment_retransmits_behind_gap_deliver_once() {
        retransmit_behind_gap(UnorderedReliableReceiver::new(), false);
    }
}
