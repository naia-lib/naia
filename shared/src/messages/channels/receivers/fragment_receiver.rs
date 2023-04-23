use std::collections::HashMap;

use log::info;

use naia_serde::BitReader;

use crate::{
    messages::fragment::{FragmentId, FragmentedMessage},
    LocalEntityAndGlobalEntityConverter, MessageContainer, MessageIndex, MessageKinds,
};

pub struct FragmentReceiver {
    current_index: MessageIndex,
    map: HashMap<FragmentId, (u32, Vec<Box<[u8]>>)>,
}

impl FragmentReceiver {
    pub fn new() -> Self {
        Self {
            current_index: 0,
            map: HashMap::new(),
        }
    }

    pub(crate) fn receive(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        message: MessageContainer,
    ) -> Option<(MessageIndex, MessageContainer)> {
        // returns a new index, 1 per full message

        // Pass right through if not a fragment
        if !message.is_fragment() {
            let output = Some((self.current_index, message));
            self.current_index = self.current_index.wrapping_add(1);
            return output;
        }

        // Message is a fragment, need to process
        let fragment = message
            .to_boxed_any()
            .downcast::<FragmentedMessage>()
            .unwrap();
        let fragment_id = fragment.id();
        let fragment_index = fragment.index();
        let fragment_total = fragment.total().as_usize();
        info!("fragment_total: {fragment_total}");
        if !self.map.contains_key(&fragment_id) {
            self.map
                .insert(fragment_id, (0, vec![Box::new([]); fragment_total]));
        }
        let (fragments_received, fragment_list) = self.map.get_mut(&fragment_id).unwrap();
        fragment_list[fragment_index.as_usize()] = fragment.to_payload();
        *fragments_received += 1;
        if *fragments_received != fragment_total as u32 {
            return None;
        }

        // we have received all fragments! put it all together
        let (_, fragment_list) = self.map.remove(&fragment_id).unwrap();
        let concat_list = fragment_list.concat();
        let mut reader = BitReader::new(&concat_list);
        let full_message_result = message_kinds.read(&mut reader, converter);
        if full_message_result.is_err() {
            // TODO: bubble up error instead of panicking here
            panic!("Cannot read fragmented message!");
        }
        let full_message = full_message_result.unwrap();
        let output = Some((self.current_index, full_message));
        self.current_index = self.current_index.wrapping_add(1);
        output
    }
}

impl FragmentReceiver {}
