use crate::{
    messages::channels::receivers::reliable_message_receiver::{
        ReceiverArranger, ReliableMessageReceiver,
    },
    types::MessageIndex,
    MessageContainer,
};

pub type UnorderedReliableReceiver = ReliableMessageReceiver<UnorderedArranger>;

impl UnorderedReliableReceiver {
    pub fn new() -> Self {
        Self::with_arranger(UnorderedArranger)
    }
}

// UnorderedArranger
pub struct UnorderedArranger;

impl ReceiverArranger for UnorderedArranger {
    fn process(
        &mut self,
        _start_message_index: MessageIndex,
        _end_message_index: MessageIndex,
        message: MessageContainer,
    ) -> Vec<MessageContainer> {
        let mut output = Vec::new();
        output.push(message);
        output
    }
}
