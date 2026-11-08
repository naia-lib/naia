//! nydus-dm9: per-observer applied-cause evidence stream.
//!
//! The causal-proof matcher (Cyberlith wiring) compares what the server
//! serialized to each observer against what the client applied. This module
//! is the shared evidence vocabulary: one ordered stream across lifecycle,
//! component and scope kinds, keyed by one identity, with coverage.
//!
//! Design notes (Ada 180894, card nydus-dm9):
//! - Delivery is not application: only actually serialized / actually
//!   applied causes are entries. Budget-deferred packets are explicit
//!   [`CauseKind::Deferred`] refusal markers, never evidence.
//! - Same-tick server send order is the scored sort, so each entry carries
//!   its `order_index` in the serialized/applied sequence. Scored order is
//!   causal only for commands on one entity (`CommandId` order); updates
//!   commute across entities (see the declared ordering below).
//! - Declared ordering (acceptance 8106884b, Ada seventh read): updates and
//!   commands compare differently. Same-tick updates commute across entities
//!   (the scorer's order is not causal): at most one per (entity, component)
//!   per tick, asserted at the drains, and matching compares the per-tick
//!   update multiset. Commands (lifecycle and scope) are causally ordered per
//!   entity by `CommandId` -- Insert-then-Remove is not Remove-then-Insert --
//!   so matching compares commands by (entity, kind, `CommandId`), commuting
//!   across entities. Within a packet updates come before commands (the wire
//!   layout), which orders an Update and a Remove of the same component in
//!   one tick by kind. `order_index` is the serialized/applied position:
//!   positional, never a causal claim; it only distinguishes repeated
//!   same-identity entries (echoes are data, never collapsed). Cross-tick
//!   order follows the tick, within the u16 wrap rule above.
//! - Ticks are `u16` and wrap; forward movement is judged within half the
//!   range. Old or ambiguous ticks never move the frontier back.
//! - `generation` is journal-owned, starting at 0 (one known world lifetime
//!   per stream). [`CauseStream::advance_generation`] emits an
//!   [`CauseKind::Epoch`] marker; Cyberlith overlays its live generation at
//!   match time. Nothing here is inferred: incomplete evidence stays absent.
//!
//! Delivery contract vs the proof deadline (Ada 180997): evidence is NOT on
//! the wire — no protocol change, no bandwidth cost. The matcher pulls both
//! streams in-process: the client drains per frame via `take_applied_causes`
//! (coverage stays cumulative), the server reads `SendState::cause_streams`.
//! Server entries are recorded at serialization under the send tick, which is
//! the tick the client applies them under — first-send and deferred
//! retransmit alike — so both sides attribute the cause to the same tick.
//! Worst-case delay for tick T's evidence is the matcher's own poll period
//! (same-tick availability) plus, for deferred causes, the retransmit lag;
//! retransmit lag is unbounded in theory and is what Mercer's lapse count
//! measures, from which the window is chosen. If wire delivery is ever
//! required, that is a protocol-change follow-up, not a silent extension.
//!
//! Opt-in contract (Ada fourth read 184483, card nydus-dm9; lapse floor and
//! once-per-tick trim per Ada fifth read 184597): the journals are OFF by
//! default on both sides — one switch in the server and client configs, no
//! push, no mark and no allocation when off. When on, each stream is bounded
//! to [`DEFAULT_CAUSE_EVIDENCE_WINDOW`] ticks behind its frontier: older
//! entries and gaps are evicted and the lapse floor raised (see
//! `Coverage.lapsed_before`), so a late reader gets "lapsed", never a false
//! gap. Refusals stay reserved for explicit [`CauseKind::Deferred`] markers.
//! Both sides drain entries through public calls while coverage stays
//! cumulative: the client via `take_applied_causes`, the server via its
//! per-observer transmit drain.

use std::collections::VecDeque;

use crate::{CommandId, ComponentKind, Tick};

/// What kind of cause an evidence entry records.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CauseKind {
    /// A component value update (echoes included: every application is data).
    Update,
    /// A component insert on an existing entity.
    Insert,
    /// A component removal from an existing entity.
    Remove,
    /// An entity spawn.
    Spawn,
    /// An entity despawn.
    Despawn,
    /// An entity entering scope (publish).
    ScopePublish,
    /// An entity leaving scope (unpublish).
    ScopeUnpublish,
    /// An explicitly deferred cause: recorded as a refusal marker so the
    /// matcher can tell "not sent" from "missing". Never evidence.
    Deferred,
    /// A world-lifetime boundary marker emitted by `advance_generation`.
    Epoch,
}

/// One evidence row in transfer shape: the matcher compares the server's
/// journal against the client's drain, so both sides project to this one
/// tuple. Updates carry `None`: the sync channels that produce them have no
/// reliable index. Commands carry their wire id under `Some`.
pub type EvidenceEntry<E> = (CauseKind, E, Option<ComponentKind>, Tick, Option<CommandId>);

/// The identity one evidence entry is keyed by: entity, component (when the
/// cause is component-scoped), the tick window it rode in on, and -- for
/// commands -- the reliable wire id. (Ada eighth read: commands match by
/// (entity, kind, `CommandId`); the tick is the window bucket, never the
/// command identity. Updates carry no wire id.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CauseIdentity<E> {
    /// The entity handle in the stream owner's space (server global entity
    /// on the server side, local entity on the client side).
    pub entity: E,
    /// The component kind, for component-scoped causes.
    pub component_kind: Option<ComponentKind>,
    /// The effective tick of the cause: the application window for updates,
    /// the first-write (or applied) tick for commands.
    pub tick: Tick,
    /// The reliable wire id, for commands only. Updates are identified by
    /// (entity, component, tick) and carry `None`.
    pub command_id: Option<CommandId>,
}

/// One applied-cause evidence entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AppliedCause<E> {
    kind: CauseKind,
    identity: CauseIdentity<E>,
    /// Position in the serialized/applied sequence. Same-tick entries in
    /// different positions are distinct causes (echoes are never collapsed).
    pub order_index: usize,
    /// The journal generation this entry belongs to.
    pub generation: u64,
}

impl<E> AppliedCause<E> {
    /// What kind of cause this entry records.
    #[must_use]
    pub fn kind(&self) -> CauseKind {
        self.kind
    }

    /// The identity this entry is keyed by.
    #[must_use]
    pub fn identity(&self) -> CauseIdentity<E>
    where
        E: Copy,
    {
        self.identity
    }
}

/// Default evidence retention window in ticks: history older than this many
/// ticks behind the frontier is evicted, naming the lapse through
/// [`Coverage::lapsed_before`]. The
/// Diax matcher window (16 ticks by default) is the natural bound.
pub const DEFAULT_CAUSE_EVIDENCE_WINDOW: u16 = 16;

/// Raise the lapse floor to `cutoff`, monotonically in tick-forward order:
/// the floor only moves forward, never back. Ticks are `u16` and wrap, so
/// forward movement is judged within half the range, matching the frontier
/// logic in `mark_tick_covered` and `note_tick`.
fn raise_lapse_floor(floor: &mut Option<Tick>, cutoff: Tick) {
    match *floor {
        None => *floor = Some(cutoff),
        Some(prev) => {
            if prev != cutoff && cutoff.wrapping_sub(prev) <= 32768 {
                *floor = Some(cutoff);
            }
        }
    }
}

/// Coverage of one evidence stream: how far it is complete, where it has
/// holes, which ticks have lapsed out of the retained window, and how much
/// it explicitly refused.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Coverage {
    /// Highest tick fully evidenced, contiguous from the stream's first tick.
    pub frontier: Option<Tick>,
    /// Ticks skipped over by later evidence (observed missing).
    pub gaps: Vec<Tick>,
    /// Lapse floor: every tick `T` with `T < lapsed_before` has lapsed out of
    /// the retained window behind the frontier. The matcher asks per tick, so
    /// a bare count cannot answer it; one bounded field can. A gap that ages
    /// out of the window folds into this floor instead of vanishing. Raised
    /// to `frontier - window` whenever the bound evicts anything.
    pub lapsed_before: Option<Tick>,
    /// Count of explicit refusals: [`CauseKind::Deferred`] markers only.
    /// Bound eviction never touches this count; it raises `lapsed_before`.
    pub refusals: u64,
}

/// One ordered applied-cause evidence stream with coverage.
#[derive(Clone, Debug, Default)]
pub struct CauseStream<E> {
    /// Application-order entries, tick-ordered on the append path. The trim
    /// pops from the front once per tick, so this is a deque, not a vec.
    entries: VecDeque<AppliedCause<E>>,
    coverage: Coverage,
    generation: u64,
    next_order: usize,
    /// Retention bound in ticks behind the frontier, or `None` for unbounded.
    /// Owners set it when evidence is enabled so memory stays flat over a
    /// long run; enforcement runs once per tick inside `mark_tick_covered`,
    /// never inside `push`.
    bound: Option<u16>,
}

impl<E> CauseStream<E> {
    /// A new empty stream at generation 0, unbounded until the owner sets a
    /// bound. Recording itself is always on at this level: the on/off switch
    /// lives with the owners (server/client configs), which gate every push
    /// and mark before they reach the stream.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            coverage: Coverage::default(),
            generation: 0,
            next_order: 0,
            bound: None,
        }
    }

    /// Set the retention bound in ticks behind the frontier (`None` lifts
    /// it). Eviction is declared, never silent: see `Coverage.lapsed_before`.
    pub fn set_bound(&mut self, bound: Option<u16>) {
        self.bound = bound;
    }

    /// The journal's current generation.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Record one cause. Returns its position in the stream.
    ///
    /// Evidence for a tick below [`Coverage::lapsed_before`] is refused: the
    /// tick already lapsed when the floor passed it, and accepting the late
    /// entry would leave it both lapsed and present. The floor already names
    /// the lapse, so the push records nothing (it still consumes an order
    /// slot, keeping positions unique). This holds for every kind, including
    /// [`CauseKind::Deferred`]: lateness is not a deferral.
    pub fn push(
        &mut self,
        kind: CauseKind,
        entity: E,
        component_kind: Option<ComponentKind>,
        tick: Tick,
        command_id: Option<CommandId>,
    ) -> usize {
        let order_index = self.next_order;
        self.next_order += 1;
        if let Some(floor) = self.coverage.lapsed_before {
            if tick != floor && floor.wrapping_sub(tick) <= 32768 {
                return order_index;
            }
        }
        if kind == CauseKind::Deferred {
            self.coverage.refusals += 1;
        } else if kind != CauseKind::Epoch {
            self.note_tick(tick);
        }
        self.entries.push_back(AppliedCause {
            kind,
            identity: CauseIdentity {
                entity,
                component_kind,
                tick,
                command_id,
            },
            order_index,
            generation: self.generation,
        });
        order_index
    }

    /// Evict history older than the bound behind the frontier, once per tick.
    /// Evicted ticks are named by raising [`Coverage::lapsed_before`] to
    /// `frontier - window`, so a matcher asking about tick `T` gets a
    /// per-tick answer (`T < lapsed_before` means lapsed). A gap that ages
    /// out folds into the floor instead of vanishing. The refusal count is
    /// untouched: it holds [`CauseKind::Deferred`] markers only. The frontier
    /// itself is never cut: the bound trims history, not the edge.
    ///
    /// Entries arrive in tick order on the append path, so the trim pops from
    /// the front; a late-heal entry for an older tick rides at the back and is
    /// evicted when it reaches the front. This runs in `mark_tick_covered`
    /// (once per tick), never in `push`: a busy observer's tick of updates
    /// costs one trim, not one scan per update.
    fn enforce_bound(&mut self) {
        let window = match self.bound {
            Some(window) => window,
            None => return,
        };
        let frontier = match self.coverage.frontier {
            Some(frontier) => frontier,
            None => return,
        };
        let cutoff = frontier.wrapping_sub(window);
        let mut evicted = false;
        while let Some(front) = self.entries.front() {
            if frontier.wrapping_sub(front.identity.tick) <= window {
                break;
            }
            self.entries.pop_front();
            evicted = true;
        }
        let before_gaps = self.coverage.gaps.len();
        self.coverage
            .gaps
            .retain(|&gap| frontier.wrapping_sub(gap) <= window);
        if self.coverage.gaps.len() != before_gaps {
            evicted = true;
        }
        if evicted {
            raise_lapse_floor(&mut self.coverage.lapsed_before, cutoff);
        }
    }

    /// All entries in application order.
    #[must_use]
    pub fn entries(&self) -> &VecDeque<AppliedCause<E>> {
        &self.entries
    }

    /// Drain and return all buffered entries, in application order. Coverage
    /// stays cumulative: the frontier, gaps, lapse floor and refusals describe
    /// every entry the stream has ever held, drained or not.
    #[must_use]
    pub fn take_entries(&mut self) -> Vec<AppliedCause<E>> {
        std::mem::take(&mut self.entries).into_iter().collect()
    }

    /// The stream's coverage: frontier, gaps, lapse floor, refusals.
    #[must_use]
    pub fn coverage(&self) -> &Coverage {
        &self.coverage
    }

    /// Mark a tick as fully covered even when it carries no cause entry: a
    /// quiet tick the sender serialized nothing for still advances the
    /// frontier, because the tick itself was processed (sent on one side,
    /// received on the other). A tick skipped over by a later mark is an
    /// observed gap — a lost packet, not a quiet tick. A late mark for a
    /// gapped tick heals the hole without moving the frontier.
    pub fn mark_tick_covered(&mut self, tick: Tick) {
        if let Some(pos) = self.coverage.gaps.iter().position(|&g| g == tick) {
            self.coverage.gaps.remove(pos);
            return;
        }
        match self.coverage.frontier {
            None => {
                self.coverage.frontier = Some(tick);
            }
            Some(frontier) => {
                let distance = tick.wrapping_sub(frontier);
                if distance == 0 {
                    // Already the frontier: re-marking is a no-op.
                } else if distance <= 32768 {
                    // Forward: every skipped tick is an observed gap.
                    let mut missing = frontier.wrapping_add(1);
                    while missing != tick {
                        self.coverage.gaps.push(missing);
                        missing = missing.wrapping_add(1);
                    }
                    self.coverage.frontier = Some(tick);
                }
                // Else: an old or half-range-ambiguous tick with no entry to
                // heal; it carries no new coverage information.
            }
        }
        self.enforce_bound();
    }

    fn note_tick(&mut self, tick: Tick) {
        // Late evidence for a skipped tick fills the hole instead of leaving
        // a gap: the tick was not missing, only late.
        if let Some(pos) = self.coverage.gaps.iter().position(|&g| g == tick) {
            self.coverage.gaps.remove(pos);
        }
        match self.coverage.frontier {
            None => {
                self.coverage.frontier = Some(tick);
            }
            Some(frontier) => {
                let distance = tick.wrapping_sub(frontier);
                if distance == 0 {
                    // Same tick: another cause (or an equal echo) in the
                    // same evidenced tick. The frontier does not move.
                } else if distance <= 32768 {
                    // Forward: every skipped tick is an observed gap.
                    let mut missing = frontier.wrapping_add(1);
                    while missing != tick {
                        self.coverage.gaps.push(missing);
                        missing = missing.wrapping_add(1);
                    }
                    self.coverage.frontier = Some(tick);
                }
                // Else: an old or half-range-ambiguous tick. It is still
                // recorded as an entry, but it never moves the frontier.
            }
        }
    }
}

impl<E: Default> CauseStream<E> {
    /// Open a new world-lifetime generation, emitting an [`CauseKind::Epoch`]
    /// marker so entries before and after stay distinguishable. The marker
    /// carries a default placeholder entity: it is a stream boundary, not a
    /// cause on any entity.
    pub fn advance_generation(&mut self) {
        self.generation += 1;
        let tick = self.coverage.frontier.unwrap_or(0);
        self.push(CauseKind::Epoch, E::default(), None, tick, None);
    }
}

/// nydus-dm9 (Ada seventh read, rule 1): at most one `Update` per
/// (entity, component) per tick. Debug-only: release pays nothing, and the
/// push path stays scan-free (see `enforce_bound`). Callers assert at their
/// drain -- the server fold per tick, the client drain per take -- so a
/// producer that double-journals trips in every debug gate.
pub fn assert_updates_unique_per_tick<E: PartialEq>(entries: &[EvidenceEntry<E>]) {
    debug_assert!(
        {
            let mut unique = true;
            for (i, candidate) in entries.iter().enumerate() {
                if candidate.0 != CauseKind::Update {
                    continue;
                }
                for seen in &entries[..i] {
                    if seen.0 == CauseKind::Update
                        && seen.1 == candidate.1
                        && seen.2 == candidate.2
                        && seen.3 == candidate.3
                    {
                        unique = false;
                        break;
                    }
                }
            }
            unique
        },
        "duplicate Update for one entity/component in one tick",
    );
}

#[cfg(test)]
mod revision_tests {
    use super::{assert_updates_unique_per_tick, CauseKind, ComponentKind, EvidenceEntry};
    use crate::{Property, Replicate};

    #[derive(Replicate)]
    struct Alpha {
        value: Property<u8>,
    }

    #[derive(Replicate)]
    struct Beta {
        value: Property<u8>,
    }

    fn update(entity: u32, kind: ComponentKind, tick: u16) -> EvidenceEntry<u32> {
        (CauseKind::Update, entity, Some(kind), tick, None)
    }

    #[test]
    fn distinct_updates_pass() {
        let alpha = ComponentKind::of::<Alpha>();
        let beta = ComponentKind::of::<Beta>();
        assert_updates_unique_per_tick(&[
            update(1, alpha, 5),
            update(2, alpha, 5),
            update(1, beta, 5),
            update(1, alpha, 6),
        ]);
    }

    #[test]
    #[should_panic(expected = "duplicate Update for one entity/component in one tick")]
    fn duplicate_update_same_tick_panics() {
        let alpha = ComponentKind::of::<Alpha>();
        assert_updates_unique_per_tick(&[update(1, alpha, 5), update(1, alpha, 5)]);
    }

    #[test]
    fn duplicate_inserts_are_echoes_not_updates() {
        // Echoes are data, never collapsed: the uniqueness rule covers
        // Updates only.
        let alpha = ComponentKind::of::<Alpha>();
        assert_updates_unique_per_tick(&[
            (CauseKind::Insert, 1, Some(alpha), 5, None),
            (CauseKind::Insert, 1, Some(alpha), 5, None),
        ]);
    }

    #[test]
    fn per_entity_command_order_is_significant() {
        // Ada seventh read, rule 2: commands on one entity are causally
        // ordered (Insert-then-Remove is not Remove-then-Insert), so the
        // journal must preserve per-entity command order for the matcher
        // to compare as a sequence. Same-kind multiset, different order:
        // collapsing to a multiset would equate them.
        use super::CauseStream;
        let mut stream = CauseStream::new();
        stream.push(CauseKind::Insert, 1u32, None, 5, Some(11));
        stream.push(CauseKind::Remove, 1u32, None, 5, Some(12));
        stream.push(CauseKind::Remove, 2u32, None, 5, Some(12));
        stream.push(CauseKind::Insert, 2u32, None, 5, Some(11));
        let drained = stream.take_entries();
        let sequence = |entity: u32| {
            drained
                .iter()
                .filter(|entry| entry.identity().entity == entity)
                .map(|entry| entry.kind())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            sequence(1),
            vec![CauseKind::Insert, CauseKind::Remove],
            "entity 1 applied Insert then Remove",
        );
        assert_eq!(
            sequence(2),
            vec![CauseKind::Remove, CauseKind::Insert],
            "entity 2 applied Remove then Insert",
        );
        assert_ne!(
            sequence(1),
            sequence(2),
            "per-entity command order must not commute: Insert-then-Remove \
             refuses to equal Remove-then-Insert",
        );
    }

    #[test]
    fn commands_carry_their_wire_id() {
        // Ada eighth read (resend identity): a command entry is keyed by
        // its reliable wire id, not by the tick it rode in on. Same
        // (entity, kind), different CommandIds: two distinct causes.
        use super::CauseStream;
        let alpha = ComponentKind::of::<Alpha>();
        let mut stream = CauseStream::new();
        stream.push(CauseKind::Insert, 1u32, Some(alpha), 5, Some(11));
        stream.push(CauseKind::Insert, 1u32, Some(alpha), 6, Some(11));
        let drained = stream.take_entries();
        assert_eq!(drained.len(), 2, "first write and resend both journal");
        assert_eq!(
            drained[0].identity().command_id,
            Some(11),
            "first write keeps the wire id it rode in on",
        );
        assert_eq!(
            drained[1].identity().command_id,
            Some(11),
            "resend keeps the SAME wire id, never a new one",
        );
        assert_ne!(
            drained[0].identity().tick,
            drained[1].identity().tick,
            "tick is the window bucket, not the identity",
        );
    }

    #[test]
    fn updates_carry_no_wire_id() {
        // Updates are identified by (entity, component, tick): no CommandId.
        use super::CauseStream;
        let alpha = ComponentKind::of::<Alpha>();
        let mut stream = CauseStream::new();
        stream.push(CauseKind::Update, 1u32, Some(alpha), 5, None);
        let drained = stream.take_entries();
        assert_eq!(drained.len(), 1);
        assert_eq!(
            drained[0].identity().command_id,
            None,
            "updates carry no wire id",
        );
    }
}
