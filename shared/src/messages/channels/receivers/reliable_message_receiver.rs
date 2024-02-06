use naia_serde::{BitReader, SerdeErr};

use crate::{GlobalRequestId, LocalEntityAndGlobalEntityConverter, LocalResponseId, MessageContainer, MessageKind, messages::{
    channels::{receivers::{
        channel_receiver::{ChannelReceiver, MessageChannelReceiver},
        fragment_receiver::FragmentReceiver,
        indexed_message_reader::IndexedMessageReader,
        reliable_receiver::ReliableReceiver,
    }, senders::request_sender::LocalRequestOrResponseId},
    message_kinds::MessageKinds,
}, RequestOrResponse, types::MessageIndex, world::remote::entity_waitlist::{EntityWaitlist, WaitlistStore}};
use crate::messages::channels::senders::request_sender::LocalRequestId;

// Receiver Arranger Trait
pub trait ReceiverArranger: Send + Sync {
    fn process(
        &mut self,
        message_index: MessageIndex,
        message: MessageContainer,
    ) -> Vec<(MessageIndex, MessageContainer)>;
}

// Reliable Receiver
pub struct ReliableMessageReceiver<A: ReceiverArranger> {
    reliable_receiver: ReliableReceiver<MessageContainer>,
    incoming_messages: Vec<MessageContainer>,
    arranger: A,
    fragment_receiver: FragmentReceiver,
    waitlist_store: WaitlistStore<(MessageIndex, MessageContainer)>,
    current_index: MessageIndex,
    incoming_requests: Vec<(MessageKind, LocalResponseId, MessageContainer)>,
    incoming_responses: Vec<(LocalRequestId, MessageContainer)>,
}

impl<A: ReceiverArranger> ReliableMessageReceiver<A> {
    pub fn with_arranger(arranger: A) -> Self {
        Self {
            reliable_receiver: ReliableReceiver::new(),
            incoming_messages: Vec::new(),
            arranger,
            fragment_receiver: FragmentReceiver::new(),
            waitlist_store: WaitlistStore::new(),
            current_index: 0,
            incoming_requests: Vec::new(),
            incoming_responses: Vec::new(),
        }
    }

    fn push_message(
        &mut self,
        message_kinds: &MessageKinds,
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        message: MessageContainer,
    ) {
        let Some(full_message) = ({
            if message.is_fragment() {
                self.fragment_receiver
                    .receive(message_kinds, converter, message)
            } else {
                Some(message)
            }
        }) else {
            return;
        };

        let first_index = self.current_index;
        self.current_index = self.current_index.wrapping_add(1);

        if let Some(entity_set) = full_message.relations_waiting() {
            //warn!("Queuing waiting message!");
            entity_waitlist.queue(
                &entity_set,
                &mut self.waitlist_store,
                (first_index, full_message),
            );
            return;
        } else {
            //info!("Received message!");
        }

        let incoming_messages = self.arranger.process(first_index, full_message);
        for (_, message) in incoming_messages {
            self.receive_message(message_kinds, converter, message);
        }
    }

    pub fn buffer_message(
        &mut self,
        message_kinds: &MessageKinds,
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        message_index: MessageIndex,
        message: MessageContainer,
    ) {
        self.reliable_receiver
            .buffer_message(message_index, message);
        let received_messages = self.reliable_receiver.receive_messages();
        for (_, received_message) in received_messages {
            self.push_message(message_kinds, entity_waitlist, converter, received_message)
        }
    }

    fn receive_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        message_container: MessageContainer
    ) {
        // look at message, see if it's a request or response
        if message_container.is_request_or_response() {
            // it is! cast it
            let request_or_response_container = message_container
                .to_boxed_any()
                .downcast::<RequestOrResponse>()
                .unwrap();
            let (local_id, request_bytes) = request_or_response_container.to_id_and_bytes();
            let mut reader = BitReader::new(&request_bytes);
            let request_or_response_result = message_kinds.read(&mut reader, converter);
            if request_or_response_result.is_err() {
                // TODO: bubble up error instead of panicking here
                panic!("Cannot read request or response message!");
            }
            let request_or_response = request_or_response_result.unwrap();

            // add it to incoming requests or responses
            match local_id {
                LocalRequestOrResponseId::Request(local_request_id) => {
                    let request = request_or_response;
                    let local_response_id = local_request_id.receive_from_remote();
                    self.incoming_requests
                        .push((request_or_response.kind(), local_response_id, request));
                }
                LocalRequestOrResponseId::Response(local_response_id) => {
                    let response = request_or_response;
                    let local_request_id = local_response_id.receive_from_remote();
                    self.incoming_responses.push((local_request_id, response));
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
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Vec<MessageContainer> {
        if let Some(list) = entity_waitlist.collect_ready_items(&mut self.waitlist_store) {
            for (first_index, mut full_message) in list {
                full_message.relations_complete(converter);
                let incoming_messages = self.arranger.process(first_index, full_message);
                for (_, message) in incoming_messages {
                    self.receive_message(message_kinds, converter, message);
                }
            }
        }

        // return buffer
        std::mem::take(&mut self.incoming_messages)
    }
}

impl<A: ReceiverArranger> MessageChannelReceiver for ReliableMessageReceiver<A> {
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let id_w_msgs = IndexedMessageReader::read_messages(message_kinds, converter, reader)?;
        for (id, message) in id_w_msgs {
            self.buffer_message(message_kinds, entity_waitlist, converter, id, message);
        }
        Ok(())
    }

    fn receive_requests_and_responses(&mut self) -> (Vec<(MessageKind, LocalResponseId, MessageContainer)>, Vec<(LocalRequestId, MessageContainer)>) {
        (std::mem::take(&mut self.incoming_requests), std::mem::take(&mut self.incoming_responses))
    }
}
