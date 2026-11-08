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

fn identities(stream: &CauseStream<u32>) -> Vec<String> {
    let mut ids: Vec<String> = stream
        .entries()
        .iter()
        .map(|entry| format!("{:?}:{:?}", entry.kind(), entry.identity()))
        .collect();
    ids.sort();
    ids
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

/// The declared commutation, updates only (acceptance 8106884b, Ada seventh
/// read): same-tick updates commute across entities -- the scorer's order is
/// not causal -- so matching compares the per-tick update multiset, never
/// position. Commands keep their per-entity sequence (see the next test).
#[test]
fn same_tick_updates_commute_matching_ignores_position() {
    let mut first = CauseStream::new();
    first.push(CauseKind::Update, 1, Some(kind()), 5);
    first.push(CauseKind::Update, 2, Some(kind()), 5);
    let mut second = CauseStream::new();
    second.push(CauseKind::Update, 2, Some(kind()), 5);
    second.push(CauseKind::Update, 1, Some(kind()), 5);

    assert_eq!(identities(&first), identities(&second));

    // ... while the two pushes genuinely landed in different positions: the
    // stream does not canonicalize order, and `order_index` is positional.
    // (`kinds()` alone cannot show this -- it discards position -- so compare
    // the identity sequence in stream order.)
    fn ordered_identities(stream: &CauseStream<u32>) -> Vec<String> {
        stream
            .entries()
            .iter()
            .map(|entry| format!("{:?}:{:?}", entry.kind(), entry.identity()))
            .collect()
    }
    assert_ne!(ordered_identities(&first), ordered_identities(&second));
}

/// Commands on one entity are causally ordered: naia sends them on the
/// reliable command channel in `CommandId` order and the client applies them
/// in that order, so Insert-then-Remove is not Remove-then-Insert. A matcher
/// must compare commands as a sequence per entity per tick: the multiset is
/// identical, so a position-blind comparison would wrongly accept the
/// reorder. The stream must keep the two orders distinguishable. Refuse.
#[test]
fn insert_then_remove_is_not_remove_then_insert() {
    let mut applied = CauseStream::new();
    applied.push(CauseKind::Insert, 1, Some(kind()), 5);
    applied.push(CauseKind::Remove, 1, Some(kind()), 5);
    let mut reordered = CauseStream::new();
    reordered.push(CauseKind::Remove, 1, Some(kind()), 5);
    reordered.push(CauseKind::Insert, 1, Some(kind()), 5);

    // Same multiset: a position-blind matcher would accept the reorder...
    assert_eq!(identities(&applied), identities(&reordered));
    // ...so the per-entity sequence must differ. Refuse.
    assert_ne!(
        kinds(&applied),
        kinds(&reordered),
        "Insert-then-Remove must stay distinguishable from Remove-then-Insert"
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

#[test]
fn quiet_tick_moves_frontier_without_gap() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    // Ticks 6 and 7 serialized nothing for this observer: they are covered
    // by the send/receive tick itself, so the frontier moves with no gap.
    stream.mark_tick_covered(6);
    stream.mark_tick_covered(7);
    let coverage = stream.coverage();
    assert_eq!(coverage.frontier, Some(7));
    assert!(coverage.gaps.is_empty());
    // A covered tick stays covered: re-marking is a no-op.
    stream.mark_tick_covered(7);
    assert_eq!(stream.coverage().frontier, Some(7));
    assert!(stream.coverage().gaps.is_empty());
}

#[test]
fn truly_lost_tick_stays_a_gap_until_seen() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    // Tick 6 never arrived (a lost packet, not a quiet tick): marking 7
    // records the hole instead of silently skipping it.
    stream.mark_tick_covered(7);
    assert_eq!(stream.coverage().frontier, Some(7));
    assert_eq!(stream.coverage().gaps, vec![6]);
    // Late arrival of the lost tick heals the hole without moving frontier.
    stream.mark_tick_covered(6);
    assert_eq!(stream.coverage().frontier, Some(7));
    assert!(stream.coverage().gaps.is_empty());
}

/// nydus-dm9 P1 follow-up (Ada 181559): a quiet stretch must arrive with
/// frontier = last tick and no gaps on BOTH sides of the wire. The server
/// marks its send tick covered per observer every send tick, even when it
/// serializes nothing (`SendState::transmit_send_job`); the client marks
/// every parsed packet tick observed and drains those marks into coverage
/// (`Client::process_all_packets`), including packets that carried no
/// entity messages. This test pins the protocol both streams follow, at
/// the stream level: an update at 5, then quiet ticks 6 and 7.
#[test]
fn quiet_stretch_advances_both_stream_frontiers_without_gaps() {
    // Server flavor: the send tick is marked covered per observer.
    let mut server = CauseStream::new();
    server.push(CauseKind::Update, 1, Some(kind()), 5);
    server.mark_tick_covered(6);
    server.mark_tick_covered(7);
    assert_eq!(server.coverage().frontier, Some(7));
    assert!(server.coverage().gaps.is_empty());

    // Client flavor: the same marks arrive via the observed-packet drain.
    let mut client = CauseStream::new();
    client.push(CauseKind::Update, 1, Some(kind()), 5);
    for tick in [6, 7] {
        client.mark_tick_covered(tick);
    }
    assert_eq!(client.coverage().frontier, Some(7));
    assert!(client.coverage().gaps.is_empty());
}

#[test]
fn late_evidence_heals_a_gap() {
    let mut stream = CauseStream::new();
    stream.push(CauseKind::Update, 1, Some(kind()), 5);
    stream.push(CauseKind::Update, 1, Some(kind()), 7);
    assert_eq!(stream.coverage().gaps, vec![6]);
    // Evidence for the skipped tick arrives late: the hole fills instead of
    // lingering as a gap, and the frontier stands where evidence led it.
    stream.push(CauseKind::Update, 2, Some(kind()), 6);
    assert!(stream.coverage().gaps.is_empty());
    assert_eq!(stream.coverage().frontier, Some(7));
}

#[test]
fn bound_evicts_old_history_and_names_the_lapse() {
    let mut stream = CauseStream::new();
    stream.set_bound(Some(16));
    for tick in 0..=20u16 {
        stream.push(CauseKind::Update, 1, Some(kind()), tick);
        stream.mark_tick_covered(tick);
    }
    let coverage = stream.coverage();
    // The frontier keeps advancing: the bound cuts history, never the edge.
    assert_eq!(coverage.frontier, Some(20));
    // Ticks 0..=3 lapsed: exactly the window behind the frontier is retained.
    assert_eq!(stream.entries().len(), 17);
    // The lapse is named per tick by the floor, never by refusal count, so
    // a quiet link under the bound still reads zero refusals.
    assert_eq!(coverage.refusals, 0);
    assert_eq!(coverage.lapsed_before, Some(4));
    // No retained entry or gap is older than the window.
    assert!(stream
        .entries()
        .iter()
        .all(|e| 20u16.wrapping_sub(e.identity().tick) <= 16));
    assert!(coverage.gaps.iter().all(|&g| 20u16.wrapping_sub(g) <= 16));
}

#[test]
fn bound_clamps_long_gap_runs_and_folds_them_into_the_floor() {
    let mut stream = CauseStream::new();
    stream.set_bound(Some(16));
    stream.push(CauseKind::Update, 1, Some(kind()), 0);
    stream.push(CauseKind::Update, 1, Some(kind()), 40);
    stream.mark_tick_covered(40);
    let coverage = stream.coverage();
    assert_eq!(coverage.frontier, Some(40));
    // Only the frontier tick's evidence is retained; the gap run is clamped
    // to the window and the aged-out gaps fold into the lapse floor.
    assert_eq!(stream.entries().len(), 1);
    assert!(coverage.gaps.len() <= 16);
    assert!(coverage.gaps.iter().all(|&g| 40u16.wrapping_sub(g) <= 16));
    assert_eq!(coverage.refusals, 0);
    assert_eq!(coverage.lapsed_before, Some(24));
}

/// Ada fifth read (184597): the trim runs once per tick on the mark, never
/// per push, so a busy observer's tick of updates costs one trim, not one
/// scan per update. Pushes alone never evict.
#[test]
fn trim_runs_on_the_tick_mark_not_on_push() {
    let mut stream = CauseStream::new();
    stream.set_bound(Some(16));
    for tick in 0..=20u16 {
        stream.push(CauseKind::Update, 1, Some(kind()), tick);
    }
    // No mark yet: pushes never trim, so all 21 entries are still held and
    // no lapse is named.
    assert_eq!(stream.entries().len(), 21);
    assert_eq!(stream.coverage().lapsed_before, None);
    stream.mark_tick_covered(20);
    // One mark trims once: exactly the window behind the frontier is kept.
    assert_eq!(stream.entries().len(), 17);
    assert_eq!(stream.coverage().lapsed_before, Some(4));
    assert_eq!(stream.coverage().refusals, 0);
}

/// Ada sixth read (184725): `push` refuses evidence for a tick below the
/// floor. The tick already lapsed when the floor passed it; accepting the
/// late entry would leave it both lapsed and present. The floor already
/// names the lapse, so the push records nothing — and a late [`CauseKind::Deferred`]
/// is refused the same way, never counted.
#[test]
fn late_evidence_below_the_floor_is_refused() {
    let mut stream = CauseStream::new();
    stream.set_bound(Some(16));
    for tick in 0..=20u16 {
        stream.push(CauseKind::Update, 1, Some(kind()), tick);
        stream.mark_tick_covered(tick);
    }
    assert_eq!(stream.coverage().lapsed_before, Some(4));
    let before = stream.entries().len();
    stream.push(CauseKind::Update, 1, Some(kind()), 2);
    stream.push(CauseKind::Deferred, 8, Some(kind()), 1);
    assert_eq!(stream.entries().len(), before);
    assert_eq!(stream.coverage().lapsed_before, Some(4));
    assert_eq!(stream.coverage().refusals, 0);
    assert!(stream.coverage().gaps.is_empty());
}

/// Ada fifth read (184597): refusals are reserved for explicit [`CauseKind::Deferred`]
/// markers. Bound eviction names the lapse through the floor instead.
#[test]
fn refusals_are_reserved_for_deferred_markers() {
    let mut stream = CauseStream::new();
    stream.set_bound(Some(16));
    stream.push(CauseKind::Deferred, 8, Some(kind()), 5);
    assert_eq!(stream.coverage().refusals, 1);
    assert_eq!(stream.coverage().lapsed_before, None);
    for tick in 6..=30u16 {
        stream.push(CauseKind::Update, 1, Some(kind()), tick);
        stream.mark_tick_covered(tick);
    }
    // Eviction moved the floor; the refusal count still holds only the one
    // deferred marker.
    assert_eq!(stream.coverage().frontier, Some(30));
    assert_eq!(stream.coverage().refusals, 1);
    assert_eq!(stream.coverage().lapsed_before, Some(14));
}
