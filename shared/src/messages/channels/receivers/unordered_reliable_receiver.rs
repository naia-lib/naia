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

    pub fn with_cap(max_messages_per_tick: Option<u16>) -> Self {
        Self::with_arranger_and_cap(UnorderedArranger, max_messages_per_tick)
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
