use crate::{
    messages::channels::reliable_receiver::{ReceiverArranger, ReliableReceiver},
    types::MessageIndex,
};

pub type UnorderedReliableReceiver<M> = ReliableReceiver<UnorderedArranger, M>;

impl<M> UnorderedReliableReceiver<M> {
    pub fn new() -> Self {
        Self::with_arranger(UnorderedArranger)
    }
}

// UnorderedArranger
pub struct UnorderedArranger;

impl<M> ReceiverArranger<M> for UnorderedArranger {
    fn process(
        &mut self,
        incoming_messages: &mut Vec<(MessageIndex, M)>,
        message_index: MessageIndex,
        message: M,
    ) {
        // TODO: if message is a Fragment, put it in a HashMap
        // if all Fragments have arrived, take it out of the HashMap and then push into incoming messages
        todo!(); // connor
        incoming_messages.push((message_index, message));
    }
}
