use std::collections::HashMap;

use naia_serde::BitReader;

use crate::{messages::fragment::{FragmentIndex, FragmentId, FragmentedMessage}, LocalEntityAndGlobalEntityConverter, MessageContainer, MessageIndex, MessageKinds};

pub struct FragmentReceiver {
    // <FragmentId, (FragmentsReceived, Option(FirstMessageIndex, FragmentCount), FragmentData)
    map: HashMap<FragmentId, (u32, Option<(MessageIndex, u32)>, Vec<Box<[u8]>>)>,
}

impl FragmentReceiver {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub(crate) fn receive(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        message_index: MessageIndex,
        message: MessageContainer,
    ) -> Option<(MessageIndex, MessageIndex, MessageContainer)> {
        if !message.is_fragment() {
            panic!("Received non-fragmented message in FragmentReceiver!");
        }

        // Message is a fragment, need to process
        let fragment = message
            .to_boxed_any()
            .downcast::<FragmentedMessage>()
            .unwrap();
        let fragment_id = fragment.id();
        let fragment_index = fragment.index();
        let fragment_total = fragment.total().as_usize();

        if !self.map.contains_key(&fragment_id) {
            self.map
                .insert(fragment_id, (0, None, vec![Box::new([]); fragment_total]));
        }
        let (fragments_received, first_message_id_opt, fragment_list) = self.map.get_mut(&fragment_id).unwrap();

        if fragment_index == FragmentIndex::zero() {
            if first_message_id_opt.is_some() {
                panic!("Received first fragment of message that already has a first fragment!");
            }
            *first_message_id_opt = Some((message_index, fragment_total as u32));
        }

        fragment_list[fragment_index.as_usize()] = fragment.to_payload();
        *fragments_received += 1;
        if *fragments_received != fragment_total as u32 {
            return None;
        }

        // we have received all fragments! put it all together
        let (_, first_index_opt, fragment_list) = self.map.remove(&fragment_id).unwrap();
        let (first_message_index, fragment_count) = first_index_opt.unwrap();
        let concat_list = fragment_list.concat();
        let mut reader = BitReader::new(&concat_list);
        let full_message_result = message_kinds.read(&mut reader, converter);
        if full_message_result.is_err() {
            // TODO: bubble up error instead of panicking here
            panic!("Cannot read fragmented message!");
        }
        let full_message = full_message_result.unwrap();
        let end_message_index = first_message_index + fragment_count as u16 - 1;
        Some((first_message_index, end_message_index, full_message))
    }
}
