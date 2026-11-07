//! nydus-dm9: per-observer applied-cause evidence stream.
//!
//! The matcher (Cyberlith wiring) tells apart six cases from this stream:
//! a missing cause, a reordered one, an equal echo, insert/remove, an epoch
//! change, and a budget-deferred packet. Each test below pins one case at the
//! evidence level: what the server serialized (or explicitly deferred), in
//! application order, with coverage. No values are stored here; values live
//! in the world snapshots this stream links.

use std::any::TypeId;

use naia_shared::{AppliedCause, CauseKind, CauseStream, ComponentKind};

fn kind() -> ComponentKind {
    ComponentKind::from(TypeId::of::<u32>())
}

fn kinds(stream: &CauseStream<u32>) -> Vec<CauseKind> {
    stream.entries().iter().map(AppliedCause::kind).collect()
}

#[test]
fn deferred_packets_are_refusals_not_evidence() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 7, Some(kind()), 5);
    stream.push(CauseKind::Deferred, 8, Some(kind()), 5);
    // The deferred entry is recorded so the matcher can tell "not sent" from
    // "missing", but it is refusal coverage: the frontier does not advance
    // past evidenced ticks on its account.
    let coverage = stream.coverage();
    assert_eq!(coverage.frontier, Some(5));
    assert_eq!(coverage.refusals, 1);
    assert_eq!(kinds(&stream), vec![CauseKind::Update, CauseKind::Deferred]);
}

#[test]
fn missing_tick_is_a_gap_not_silent() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 1);
    stream.push(CauseKind::Update, 1, Some(kind()), 2);
    stream.push(CauseKind::Update, 1, Some(kind()), 4);
    let coverage = stream.coverage();
    assert_eq!(coverage.frontier, Some(4));
    assert_eq!(coverage.gaps, vec![3]);
}

#[test]
fn cross_kind_application_order_is_preserved() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    stream.push(CauseKind::Insert, 2, Some(kind()), 5);
    stream.push(CauseKind::ScopePublish, 3, None, 5);
    // Same tick, three kinds: the stream keeps application order so a
    // reordered cause is detectable by position, not by kind grouping.
    let entries = stream.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].order_index, 0);
    assert_eq!(entries[1].order_index, 1);
    assert_eq!(entries[2].order_index, 2);
    assert_eq!(
        kinds(&stream),
        vec![
            CauseKind::Update,
            CauseKind::Insert,
            CauseKind::ScopePublish
        ]
    );
}

#[test]
fn equal_echoes_are_both_recorded() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    // Same identity, same tick, same position in kind grouping: still two
    // entries. An echo is data, not a duplicate to collapse.
    let entries = stream.entries();
    assert_eq!(entries.len(), 2);
    assert_ne!(entries[0].order_index, entries[1].order_index);
    assert_eq!(stream.coverage().frontier, Some(5));
    assert!(stream.coverage().gaps.is_empty());
}

#[test]
fn insert_remove_are_distinct_from_update() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    stream.push(CauseKind::Insert, 1, Some(kind()), 5);
    stream.push(CauseKind::Remove, 1, Some(kind()), 5);
    assert_eq!(
        kinds(&stream),
        vec![CauseKind::Update, CauseKind::Insert, CauseKind::Remove]
    );
}

#[test]
fn epoch_change_distinguishes_entries() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    stream.advance_generation();
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    let entries = stream.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].generation, 0);
    assert_eq!(entries[1].kind(), CauseKind::Epoch);
    assert_eq!(entries[1].generation, 1);
    assert_eq!(entries[2].generation, 1);
}

#[test]
fn old_ticks_do_not_move_frontier_back() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 8);
    stream.push(CauseKind::Update, 1, Some(kind()), 8);
    stream.push(CauseKind::Update, 1, Some(kind()), 6);
    let coverage = stream.coverage();
    assert_eq!(coverage.frontier, Some(8));
    assert!(coverage.gaps.is_empty());
}
