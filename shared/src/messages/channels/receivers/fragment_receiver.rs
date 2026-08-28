use std::collections::HashMap;

use log::warn;
use naia_serde::BitReader;

use crate::{
    messages::fragment::{FragmentId, FragmentedMessage},
    LocalEntityAndGlobalEntityConverter, MessageContainer, MessageIndex, MessageKinds,
};

/// One in-flight fragment sequence: the declared total, the message index of
/// fragment 0 once it arrives, and the payloads received so far keyed by index.
///
/// Payloads live in a map rather than a pre-sized `Vec` on purpose. `total` is
/// read off the wire, so pre-sizing let a peer make the server allocate a slot
/// per fragment of a sequence it never intends to send -- up to
/// `FRAGMENT_INDEX_LIMIT` slots per id, across as many ids as it cares to open.
/// Keyed storage costs only what actually arrived.
struct FragmentEntry {
    total: u32,
    first_message_index: Option<MessageIndex>,
    payloads: HashMap<u32, Box<[u8]>>,
}

pub struct FragmentReceiver {
    map: HashMap<FragmentId, FragmentEntry>,
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
        // Callers gate on `is_fragment()`, so reaching here with anything else is
        // a local routing bug, not remote input.
        if !message.is_fragment() {
            panic!("Received non-fragmented message in FragmentReceiver!");
        }

        let fragment = message
            .to_boxed_any()
            .downcast::<FragmentedMessage>()
            .expect("message reported itself as a fragment");
        let fragment_id = fragment.id();
        let fragment_index = fragment.index().as_usize() as u32;
        let fragment_total = fragment.total().as_usize() as u32;

        // Every field below came off the wire, so a peer picks all of them. Each
        // check here guards a slot that previously panicked or indexed out of
        // bounds; the sequence is dropped rather than the process.
        if fragment_total == 0 {
            warn!("Discarding fragment (id={fragment_id:?}) declaring a total of 0.");
            return None;
        }
        if fragment_index >= fragment_total {
            warn!(
                "Discarding fragment (id={fragment_id:?}) with index {fragment_index} beyond its declared total {fragment_total}."
            );
            return None;
        }

        let entry = self
            .map
            .entry(fragment_id)
            .or_insert_with(|| FragmentEntry {
                total: fragment_total,
                first_message_index: None,
                payloads: HashMap::new(),
            });

        // A later fragment disagreeing with the total the sequence was opened
        // with cannot be reassembled into anything coherent.
        if entry.total != fragment_total {
            warn!(
                "Discarding fragment (id={fragment_id:?}) declaring total {fragment_total}, but its sequence was opened with total {}.",
                entry.total
            );
            return None;
        }

        if fragment_index == 0 {
            if entry.first_message_index.is_some() {
                warn!("Discarding repeated first fragment (id={fragment_id:?}).");
                return None;
            }
            entry.first_message_index = Some(message_index);
        }

        // Counting insertions rather than arrivals: repeating one index used to
        // drive the count up to the total without the sequence being complete.
        if entry
            .payloads
            .insert(fragment_index, fragment.to_payload())
            .is_some()
        {
            warn!("Discarding repeated fragment index {fragment_index} (id={fragment_id:?}).");
            return None;
        }

        if entry.payloads.len() as u32 != entry.total {
            return None;
        }

        // Complete. Every index is distinct and below `total`, so the sequence
        // covers 0..total exactly and reassembles in index order.
        let entry = self.map.remove(&fragment_id).unwrap();
        let Some(first_message_index) = entry.first_message_index else {
            warn!("Discarding complete fragment sequence (id={fragment_id:?}) missing its first fragment.");
            return None;
        };
        let mut payloads = entry.payloads;
        let mut concat_list: Vec<u8> = Vec::new();
        for index in 0..entry.total {
            if let Some(payload) = payloads.remove(&index) {
                concat_list.extend_from_slice(&payload);
            }
        }

        let mut reader = BitReader::new(&concat_list);
        let full_message = match message_kinds.read(&mut reader, converter) {
            Ok(msg) => msg,
            Err(e) => {
                // Reassembled bytes are unreadable — peer sent a malformed fragmented
                // message. Discard the whole sequence rather than crashing.
                warn!(
                    "Discarding malformed reassembled fragment (id={:?}, {}); dropping message.",
                    fragment_id, e
                );
                return None;
            }
        };
        let end_message_index = first_message_index.wrapping_add(entry.total as u16 - 1);
        Some((first_message_index, end_message_index, full_message))
    }
}

#[cfg(test)]
mod tests {
    use naia_derive::MessageInternal;

    use super::FragmentReceiver;
    use crate::{
        messages::fragment::{FragmentId, FragmentIndex, FragmentedMessage},
        FakeEntityConverter, MessageContainer, MessageKinds,
    };

    #[derive(MessageInternal)]
    pub struct StringMessage {
        pub inner: String,
    }

    /// Every field of a `FragmentedMessage` -- id, index, total, bytes -- is read
    /// off the wire, so a connected peer chooses all of them freely.
    fn fragment(index: u32, total: u32) -> MessageContainer {
        let mut msg = FragmentedMessage::new(
            FragmentId::zero(),
            FragmentIndex::from_u32(index),
            Box::new([0u8; 4]),
        );
        msg.set_total(FragmentIndex::from_u32(total));
        MessageContainer::new(Box::new(msg))
    }

    fn receive(receiver: &mut FragmentReceiver, msg: MessageContainer, idx: u16) {
        let kinds = MessageKinds::new();
        receiver.receive(&kinds, &FakeEntityConverter, idx, msg);
    }

    /// A fragment whose index is past its own declared total indexes straight
    /// past the end of the reassembly buffer.
    #[test]
    fn fragment_index_beyond_total_is_dropped() {
        let mut receiver = FragmentReceiver::new();
        receive(&mut receiver, fragment(100, 1), 0);
    }

    /// Two fragments claiming index 0 are a malformed sequence, not a local bug.
    #[test]
    fn duplicate_first_fragment_is_dropped() {
        let mut receiver = FragmentReceiver::new();
        receive(&mut receiver, fragment(0, 2), 0);
        receive(&mut receiver, fragment(0, 2), 1);
    }

    /// Repeating one non-zero index can drive the received-count up to the total
    /// without fragment 0 ever arriving.
    #[test]
    fn duplicate_indices_do_not_complete_a_sequence() {
        let mut receiver = FragmentReceiver::new();
        receive(&mut receiver, fragment(1, 2), 0);
        receive(&mut receiver, fragment(1, 2), 1);
    }

    /// A declared total of zero leaves an empty buffer that any index overruns.
    #[test]
    fn zero_total_is_dropped() {
        let mut receiver = FragmentReceiver::new();
        receive(&mut receiver, fragment(0, 0), 0);
    }

    /// Reassembly must still work, and must not depend on arrival order -- the
    /// payloads are keyed by index now rather than written into a pre-sized Vec.
    #[test]
    fn a_real_fragmented_message_reassembles_in_any_order() {
        use crate::messages::channels::senders::message_fragmenter::MessageFragmenter;
        use crate::Protocol;

        let mut protocol = Protocol::builder();
        protocol.add_message::<StringMessage>();
        let protocol = protocol.build();
        let kinds = &protocol.message_kinds;

        let original = StringMessage {
            inner: "x".repeat(4096),
        };

        for reverse in [false, true] {
            let mut fragmenter = MessageFragmenter::new();
            let mut fragments = fragmenter.fragment_message(
                kinds,
                &mut FakeEntityConverter,
                MessageContainer::new(Box::new(original.clone())),
            );
            assert!(fragments.len() > 1, "test payload must actually fragment");
            if reverse {
                fragments.reverse();
            }

            let mut receiver = FragmentReceiver::new();
            let last = fragments.len() - 1;
            let mut result = None;
            for (i, fragment) in fragments.into_iter().enumerate() {
                let out = receiver.receive(kinds, &FakeEntityConverter, i as u16, fragment);
                if i == last {
                    result = out;
                }
            }
            let (_, _, message) = result.expect("sequence should reassemble");
            let rebuilt = message
                .to_boxed_any()
                .downcast::<StringMessage>()
                .expect("reassembled into the original type");
            assert_eq!(rebuilt.inner, original.inner);
        }
    }
}
