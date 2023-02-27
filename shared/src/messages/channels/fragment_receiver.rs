use crate::{messages::message_fragmenter::FragmentedMessage, Message, MessageIndex};

pub trait IsFragment {
    fn is_fragment(&self) -> bool;
    fn to_fragment(self) -> Option<FragmentedMessage>;
}

impl IsFragment for Box<dyn Message> {
    fn is_fragment(&self) -> bool {
        <dyn Message>::is_fragment(self.as_ref())
    }

    fn to_fragment(self) -> Option<FragmentedMessage> {
        let boxed_any = self.to_boxed_any();
        let Ok(msg) = boxed_any.downcast::<FragmentedMessage>() else {
            return None;
        };
        Some(*msg)
    }
}

pub struct FragmentReceiver {}

impl FragmentReceiver {
    pub(crate) fn receive<M: IsFragment>(
        &self,
        index: MessageIndex,
        message: M,
    ) -> Option<(MessageIndex, M)> {
        // returns a new index, 1 per full message
        todo!()
    }
}

impl FragmentReceiver {
    pub fn new() -> Self {
        Self {}
    }
}
