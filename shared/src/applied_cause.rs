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
//! - Same-tick server send order is priority-commuted (the scored sort), so
//!   each entry carries its `order_index` in the serialized/applied sequence.
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
//! Opt-in contract (Ada fourth read 184483, card nydus-dm9): the
//! journals are OFF by default on both sides — one switch in the server and
//! client configs, no push, no mark and no allocation when off. When on, each
//! stream is bounded to [`DEFAULT_CAUSE_EVIDENCE_WINDOW`] ticks behind its
//! frontier: older entries and gaps are evicted, each eviction declared as one
//! refusal (see `Coverage.refusals`), so a late reader gets "lapsed", never a
//! false gap. Both sides drain entries through public calls while coverage
//! stays cumulative: the client via `take_applied_causes`, the server via its
//! per-observer transmit drain.

use crate::{ComponentKind, Tick};

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

/// The identity one evidence entry is keyed by: entity, component (when the
/// cause is component-scoped), and the effective tick of the cause.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CauseIdentity<E> {
    /// The entity handle in the stream owner's space (server global entity
    /// on the server side, local entity on the client side).
    pub entity: E,
    /// The component kind, for component-scoped causes.
    pub component_kind: Option<ComponentKind>,
    /// The effective tick of the cause.
    pub tick: Tick,
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
/// ticks behind the frontier is evicted, declared as a refusal lapse. The
/// Diax matcher window (16 ticks by default) is the natural bound.
pub const DEFAULT_CAUSE_EVIDENCE_WINDOW: u16 = 16;

/// Coverage of one evidence stream: how far it is complete, where it has
/// holes, and how much it explicitly refused.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Coverage {
    /// Highest tick fully evidenced, contiguous from the stream's first tick.
    pub frontier: Option<Tick>,
    /// Ticks skipped over by later evidence (observed missing).
    pub gaps: Vec<Tick>,
    /// Count of explicit refusals: [`CauseKind::Deferred`] markers plus one
    /// per bound enforcement that evicted history (the lapse declaration).
    pub refusals: u64,
}

/// One ordered applied-cause evidence stream with coverage.
#[derive(Clone, Debug, Default)]
pub struct CauseStream<E> {
    entries: Vec<AppliedCause<E>>,
    coverage: Coverage,
    generation: u64,
    next_order: usize,
    /// Retention bound in ticks behind the frontier, or `None` for unbounded.
    /// Owners set it when evidence is enabled so memory stays flat over a
    /// long run; enforcement runs inside `push` and `mark_tick_covered`.
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
            entries: Vec::new(),
            coverage: Coverage::default(),
            generation: 0,
            next_order: 0,
            bound: None,
        }
    }

    /// Set the retention bound in ticks behind the frontier (`None` lifts
    /// it). Eviction is declared, never silent: see `Coverage.refusals`.
    pub fn set_bound(&mut self, bound: Option<u16>) {
        self.bound = bound;
    }

    /// The journal's current generation.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Record one cause. Returns its position in the stream.
    pub fn push(
        &mut self,
        kind: CauseKind,
        entity: E,
        component_kind: Option<ComponentKind>,
        tick: Tick,
    ) -> usize {
        let order_index = self.next_order;
        self.next_order += 1;
        if kind == CauseKind::Deferred {
            self.coverage.refusals += 1;
        } else if kind != CauseKind::Epoch {
            self.note_tick(tick);
        }
        self.entries.push(AppliedCause {
            kind,
            identity: CauseIdentity {
                entity,
                component_kind,
                tick,
            },
            order_index,
            generation: self.generation,
        });
        self.enforce_bound();
        order_index
    }

    /// Evict history older than the bound behind the frontier. One refusal
    /// is declared per enforcement that drops anything — entries or gaps —
    /// so a matcher reading later gets "lapsed", never a false gap. The
    /// frontier itself is never cut: the bound trims history, not the edge.
    fn enforce_bound(&mut self) {
        let window = match self.bound {
            Some(window) => window,
            None => return,
        };
        let frontier = match self.coverage.frontier {
            Some(frontier) => frontier,
            None => return,
        };
        let before_entries = self.entries.len();
        self.entries
            .retain(|entry| frontier.wrapping_sub(entry.identity.tick) <= window);
        let before_gaps = self.coverage.gaps.len();
        self.coverage
            .gaps
            .retain(|&gap| frontier.wrapping_sub(gap) <= window);
        if self.entries.len() != before_entries || self.coverage.gaps.len() != before_gaps {
            self.coverage.refusals += 1;
        }
    }

    /// All entries in application order.
    #[must_use]
    pub fn entries(&self) -> &[AppliedCause<E>] {
        &self.entries
    }

    /// Drain and return all buffered entries, in application order. Coverage
    /// stays cumulative: the frontier, gaps and refusals describe every
    /// entry the stream has ever held, drained or not.
    #[must_use]
    pub fn take_entries(&mut self) -> Vec<AppliedCause<E>> {
        std::mem::take(&mut self.entries)
    }

    /// The stream's coverage: frontier, gaps, refusals.
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
        self.push(CauseKind::Epoch, E::default(), None, tick);
    }
}
