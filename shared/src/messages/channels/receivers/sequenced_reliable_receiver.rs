use crate::{
    messages::channels::receivers::reliable_message_receiver::{
        ReceiverArranger, ReliableMessageReceiver,
    },
    sequence_less_than,
    types::MessageIndex,
    MessageContainer,
};

pub type SequencedReliableReceiver = ReliableMessageReceiver<SequencedArranger>;

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
        message_index: MessageIndex,
        message: MessageContainer,
    ) -> Vec<(MessageIndex, MessageContainer)> {
        let mut output = Vec::new();
        if !sequence_less_than(message_index, self.newest_received_message_index) {
            self.newest_received_message_index = message_index;
            output.push((message_index, message));
        }
        output
    }
}
