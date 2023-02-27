use crate::{
    messages::channels::{
        fragment_receiver::IsFragment,
        reliable_receiver::{ReceiverArranger, ReliableReceiver},
    },
    sequence_less_than,
    types::MessageIndex,
};

pub type SequencedReliableReceiver<M> = ReliableReceiver<SequencedArranger, M>;

impl<M: IsFragment> SequencedReliableReceiver<M> {
    pub fn new() -> Self {
        Self::with_arranger(SequencedArranger {
            newest_received_message_index: 0,
        })
    }
}

// SequencedArranger
pub struct SequencedArranger {
    newest_received_message_index: MessageIndex,
}

impl<M> ReceiverArranger<M> for SequencedArranger {
    fn process(
        &mut self,
        incoming_messages: &mut Vec<(MessageIndex, M)>,
        message_index: MessageIndex,
        message: M,
    ) {
        if !sequence_less_than(message_index, self.newest_received_message_index) {
            self.newest_received_message_index = message_index;
            incoming_messages.push((message_index, message));
        }
    }
}
