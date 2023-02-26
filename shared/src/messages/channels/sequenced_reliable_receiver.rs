use crate::{
    messages::channels::reliable_receiver::{ReceiverArranger, ReliableReceiver},
    sequence_less_than,
    types::MessageIndex,
};

pub type SequencedReliableReceiver<M> = ReliableReceiver<SequencedArranger, M>;

impl<M> SequencedReliableReceiver<M> {
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
        // TODO: if message is a Fragment, put it in a HashMap
        // if all Fragments have arrived, take it out of the HashMap and then push into incoming messages
        // only at that point should you increment the 'newest_received_message_index'
        // if the newest_received_message_index is greater than the incoming fragment ... we've already received
        // another full message further down the line, discard it

        // note: fragmented messages compete to arrive first here

        todo!(); // connor

        if !sequence_less_than(message_index, self.newest_received_message_index) {
            self.newest_received_message_index = message_index;
            incoming_messages.push((message_index, message));
        }
    }
}
