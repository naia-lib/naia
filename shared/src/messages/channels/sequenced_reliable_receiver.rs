use crate::{
    messages::channels::reliable_receiver::{ReceiverArranger, ReliableReceiver},
    sequence_less_than,
    types::MessageIndex,
    MessageContainer,
};

pub type SequencedReliableReceiver = ReliableReceiver<SequencedArranger>;

impl SequencedReliableReceiver {
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

impl ReceiverArranger for SequencedArranger {
    fn process(
        &mut self,
        incoming_messages: &mut Vec<(MessageIndex, MessageContainer)>,
        message_index: MessageIndex,
        message: MessageContainer,
    ) {
        if !sequence_less_than(message_index, self.newest_received_message_index) {
            self.newest_received_message_index = message_index;
            incoming_messages.push((message_index, message));
        }
    }
}
