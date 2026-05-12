use std::{any::Any, collections::HashSet};

use naia_serde::BitWrite;

use crate::{
    messages::{
        channels::receivers::{
            ordered_reliable_receiver::OrderedArranger,
            sequenced_reliable_receiver::SequencedArranger,
            ReceiverArranger,
        },
        message::Message,
        message_kinds::{MessageKind, MessageKinds},
    },
    named::Named,
    world::entity::entity_converters::LocalEntityAndGlobalEntityConverterMut,
    LocalEntityAndGlobalEntityConverter, MessageBuilder, MessageContainer, RemoteEntity,
};

// --- StubMessage -----------------------------------------------------------

#[derive(Clone)]
struct StubMessage(u32);

impl Named for StubMessage {
    fn name(&self) -> String {
        "StubMessage".into()
    }
    fn protocol_name() -> &'static str {
        "StubMessage"
    }
}

impl Message for StubMessage {
    fn kind(&self) -> MessageKind {
        MessageKind::of::<Self>()
    }
    fn to_boxed_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
    fn create_builder() -> Box<dyn MessageBuilder>
    where
        Self: Sized,
    {
        unimplemented!("not used in tests")
    }
    fn bit_length(
        &self,
        _: &MessageKinds,
        _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) -> u32 {
        0
    }
    fn is_fragment(&self) -> bool {
        false
    }
    fn is_request(&self) -> bool {
        false
    }
    fn write(
        &self,
        _: &MessageKinds,
        _: &mut dyn BitWrite,
        _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
    }
    fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>> {
        None
    }
    fn relations_complete(&mut self, _: &dyn LocalEntityAndGlobalEntityConverter) {}
}

fn stub(id: u32) -> MessageContainer {
    MessageContainer::new(Box::new(StubMessage(id)))
}

fn extract(mc: MessageContainer) -> u32 {
    mc.to_boxed_any().downcast::<StubMessage>().unwrap().0
}

// Process a single-slot message (start == end) and return the extracted IDs.
fn ordered_send(arr: &mut OrderedArranger, idx: u16, id: u32) -> Vec<u32> {
    arr.process(idx, idx, stub(id)).into_iter().map(extract).collect()
}

fn sequenced_send(arr: &mut SequencedArranger, idx: u16, id: u32) -> Vec<u32> {
    arr.process(idx, idx, stub(id)).into_iter().map(extract).collect()
}

// ---------------------------------------------------------------------------
// Ordered: deterministic unit tests
// ---------------------------------------------------------------------------

#[test]
fn ordered_in_order_delivery() {
    let mut arr = OrderedArranger::new();
    for i in 0u16..8 {
        let out = ordered_send(&mut arr, i, i as u32);
        assert_eq!(out, vec![i as u32], "idx={i}");
    }
}

#[test]
fn ordered_reverse_delivery() {
    let mut arr = OrderedArranger::new();
    let n: u16 = 4;
    for i in (0..n).rev() {
        let out = ordered_send(&mut arr, i, i as u32);
        if i == 0 {
            assert_eq!(out, (0..n as u32).collect::<Vec<_>>());
        } else {
            assert!(out.is_empty(), "idx={i} should buffer");
        }
    }
}

#[test]
fn ordered_interleaved_delivery() {
    let mut arr = OrderedArranger::new();
    assert!(ordered_send(&mut arr, 1, 10).is_empty());
    assert!(ordered_send(&mut arr, 3, 30).is_empty());
    assert_eq!(ordered_send(&mut arr, 0, 0), vec![0, 10]);
    assert_eq!(ordered_send(&mut arr, 2, 20), vec![20, 30]);
}

#[test]
fn ordered_wraparound() {
    let mut arr = OrderedArranger::new();
    let start: u16 = u16::MAX - 3;
    for i in 0u16..start {
        let out = ordered_send(&mut arr, i, i as u32);
        assert_eq!(out.len(), 1, "fast-forward idx={i}");
    }
    // out-of-order across the u16 boundary
    let arrivals: &[(u16, u32)] = &[
        (start.wrapping_add(1), 101),
        (start.wrapping_add(3), 103),
        (start, 100),
        (start.wrapping_add(2), 102),
    ];
    let mut all_out: Vec<u32> = Vec::new();
    for &(idx, id) in arrivals {
        all_out.extend(ordered_send(&mut arr, idx, id));
    }
    assert_eq!(all_out, vec![100, 101, 102, 103]);
}

// ---------------------------------------------------------------------------
// Sequenced: deterministic unit tests
// ---------------------------------------------------------------------------

#[test]
fn sequenced_in_order_accepts_all() {
    let mut arr = SequencedArranger::new();
    for i in 0u16..8 {
        let out = sequenced_send(&mut arr, i, i as u32);
        assert_eq!(out, vec![i as u32], "idx={i}");
    }
}

#[test]
fn sequenced_drops_older() {
    let mut arr = SequencedArranger::new();
    assert_eq!(sequenced_send(&mut arr, 5, 50), vec![50]);
    for i in 0u16..5 {
        assert!(sequenced_send(&mut arr, i, i as u32).is_empty(), "stale idx={i}");
    }
}

#[test]
fn sequenced_equal_index_passes_through() {
    let mut arr = SequencedArranger::new();
    assert_eq!(sequenced_send(&mut arr, 3, 30), vec![30]);
    // equal index: sequence_less_than(3, 3) == false → not stale
    assert_eq!(sequenced_send(&mut arr, 3, 31), vec![31]);
}

#[test]
fn sequenced_wraparound() {
    // Advance in two large jumps to avoid the "past half" zone relative to newest=0.
    // sequence_less_than(u16::MAX, 0) == true, so u16::MAX is stale from a fresh arranger;
    // we must first advance newest to a value that makes u16::MAX in the future half.
    let mut arr = SequencedArranger::new();
    assert_eq!(sequenced_send(&mut arr, 32767, 1), vec![1]); // newest = 32767
    assert_eq!(sequenced_send(&mut arr, 65534, 2), vec![2]); // newest = u16::MAX - 1
    assert_eq!(sequenced_send(&mut arr, u16::MAX, 3), vec![3]); // newest = u16::MAX
    // 0 is newer than u16::MAX in wrapping sequence space
    assert_eq!(sequenced_send(&mut arr, 0, 4), vec![4]); // newest = 0
    // u16::MAX is now stale (behind 0 in wrapping space)
    assert!(sequenced_send(&mut arr, u16::MAX, 5).is_empty());
}

// ---------------------------------------------------------------------------
// Proptest: OrderedArranger
// ---------------------------------------------------------------------------

#[cfg(test)]
mod ordered_prop {
    use super::*;
    use proptest::prelude::*;

    fn make_permutation(n: usize, seed: u64) -> Vec<u16> {
        let mut indices: Vec<u16> = (0..n as u16).collect();
        let mut s = seed;
        for i in (1..n).rev() {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = (s as usize) % (i + 1);
            indices.swap(i, j);
        }
        indices
    }

    proptest! {
        // All n messages arrive in a random permutation; output must be exactly [0..n].
        #[test]
        fn prop_ordered_delivers_all_in_order(n in 1usize..=16, seed in 0u64..10_000) {
            let perm = make_permutation(n, seed);
            let mut arr = OrderedArranger::new();
            let mut all_out: Vec<u32> = Vec::new();
            for &idx in &perm {
                all_out.extend(ordered_send(&mut arr, idx, idx as u32));
            }
            let expected: Vec<u32> = (0..n as u32).collect();
            prop_assert_eq!(all_out, expected);
        }
    }

    proptest! {
        // At every delivery step the accumulated output is a contiguous prefix [0..k].
        #[test]
        fn prop_ordered_output_is_contiguous_prefix(n in 2usize..=16, seed in 0u64..10_000) {
            let perm = make_permutation(n, seed);
            let mut arr = OrderedArranger::new();
            let mut next_expected: u32 = 0;
            for &idx in &perm {
                let out = ordered_send(&mut arr, idx, idx as u32);
                for id in out {
                    prop_assert_eq!(id, next_expected);
                    next_expected += 1;
                }
            }
            prop_assert_eq!(next_expected, n as u32);
        }
    }
}

// ---------------------------------------------------------------------------
// Proptest: SequencedArranger
// ---------------------------------------------------------------------------

#[cfg(test)]
mod sequenced_prop {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // Any arrival sequence → emitted indices are monotone non-decreasing in sequence space.
        #[test]
        fn prop_sequenced_output_is_monotone(
            arrivals in prop::collection::vec(0u16..128, 1..=32),
        ) {
            let mut arr = SequencedArranger::new();
            let mut last: Option<u16> = None;
            for &idx in &arrivals {
                let out = arr.process(idx, idx, stub(idx as u32));
                for mc in out {
                    let id = extract(mc) as u16;
                    if let Some(prev) = last {
                        prop_assert!(
                            !crate::sequence_less_than(id, prev),
                            "non-monotone: emitted {} after {}",
                            id,
                            prev
                        );
                    }
                    last = Some(id);
                }
            }
        }
    }

    proptest! {
        // After a high-index message, all lower indices are dropped.
        #[test]
        fn prop_sequenced_drops_stale_batch(
            high in 10u16..200,
            count in 1usize..=10,
        ) {
            let mut arr = SequencedArranger::new();
            let out = arr.process(high, high, stub(high as u32));
            prop_assert_eq!(out.len(), 1);
            for i in 0u16..count as u16 {
                let idx = high.wrapping_sub(1).wrapping_sub(i);
                let out = arr.process(idx, idx, stub(idx as u32));
                prop_assert!(out.is_empty(), "stale idx={} should be dropped", idx);
            }
        }
    }
}
