use crate::{
    messages::channels::{
        fragment_receiver::IsFragment,
        reliable_receiver::{ReceiverArranger, ReliableReceiver},
    },
    types::MessageIndex,
};

pub type UnorderedReliableReceiver<M> = ReliableReceiver<UnorderedArranger, M>;

impl<M: IsFragment> UnorderedReliableReceiver<M> {
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
        incoming_messages.push((message_index, message));
    }
}
