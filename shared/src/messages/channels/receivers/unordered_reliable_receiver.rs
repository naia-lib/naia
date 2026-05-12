use crate::{
    messages::channels::receivers::reliable_message_receiver::{
        ReceiverArranger, ReliableMessageReceiver,
    },
    types::MessageIndex,
    MessageContainer,
};

/// Reliable receiver that delivers messages to callers as soon as they arrive, without ordering.
pub type UnorderedReliableReceiver = ReliableMessageReceiver<UnorderedArranger>;

impl UnorderedReliableReceiver {
    /// Creates a new `UnorderedReliableReceiver` with no throughput cap.
    pub fn new() -> Self {
        Self::with_arranger(UnorderedArranger)
    }

    /// Creates a new `UnorderedReliableReceiver` capped at `max_messages_per_tick` deliveries per tick.
    pub fn with_cap(max_messages_per_tick: Option<u16>) -> Self {
        Self::with_arranger_and_cap(UnorderedArranger, max_messages_per_tick)
    }
}

/// Arranger that passes every message through immediately in arrival order.
pub struct UnorderedArranger;

impl ReceiverArranger for UnorderedArranger {
    fn process(
        &mut self,
        _start_message_index: MessageIndex,
        _end_message_index: MessageIndex,
        message: MessageContainer,
    ) -> Vec<MessageContainer> {
        vec![message]
    }
}
