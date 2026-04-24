# Sidequest — Priority Accumulator

**Status:** 🔎 research & planning (opened 2026-04-24, between Phase 4 and Phase 5 of `BENCH_PERF_UPGRADE.md`)
**Owner:** Connor + Claude (Opus 4.7)
**Origin:** Long-standing Naia backlog item. Surfaced back to the top by the Phase 4.5 mutable resend-window spike (`idle_distribution.rs` shows ≥2700× max/p50 on every `N=10000 mut` cell at reliable-sender resend cadence).

---

## Thesis

Naia does not currently pace outbound work by **bandwidth budget** or **per-item priority**. Every tick, each peer (server OR client) writes whatever the per-channel senders hand it, in whatever order they hand it, at whatever rate the protocol accepts. This is **a sender-side problem, not a server-only problem** — Naia supports client-authoritative messages, requests, responses, and client-owned entities, so the same pathology is available on the client-outbound path. This is fine when volume is tiny, but it breaks down as soon as either peer has:

- Spawn bursts (10K entity-spawn commands queued into `UnorderedReliable` at level-load).
- Resend storms (RTT-driven retransmits of 10K unacked commands happening on the same tick).
- Wide component-update fan-out (one moving unit × 64 users × 5 dirty components × each tick).
- Mixed message traffic (requests/responses competing with game-state updates for MTU space).

Glenn Fiedler's **priority accumulator** is the canonical solution. Each replicated item (entity, component, message) has a **priority** that accumulates every tick, is selected and sent up to a per-tick **bandwidth budget**, and then its accumulator is reset. Items stay "in line" across ticks instead of being shoved through in one burst; the send side naturally self-paces under load.

Connor's long-standing view: this is **an absolute necessity for Naia to be production-ready**. Without it, every Naia server has an implicit "don't do anything bursty" constraint that games routinely violate.

---

## Why this sidequest interrupts the 7-phase perf plan

Phase 4.5 (mutable resend-window spike) is a blocker for Phase 5. The priority accumulator is the **natural fix** for that spike because it replaces "send all N due resends on tick T" with "spend B bytes of bandwidth on the highest-priority due resends this tick, carry the rest into T+1." The spike stops existing — it becomes steady-state load spread across ticks.

If research confirms this, Phase 4.5 folds into the sidequest and we implement both together. If research shows the spike has a different root cause, Phase 4.5 still runs independently.

Either way, the priority accumulator is bigger than one spike — it reshapes:

- Component update dispatch
- `UnorderedReliable` / `OrderedReliable` entity-command channel
- Plain Naia messages (`ChannelSender`)
- Request/response (built on messages)

So this sidequest is scoped to survey *all four surfaces* and produce a unified plan, not a point fix for one channel.

---

## Questions the research stage must answer

### Fiedler's concept (source material distillation)
- What precisely is a priority accumulator? (one-paragraph definition, not vibes.)
- What are its inputs — priority function, tick rate, bandwidth budget, item state?
- What bandwidth accounting model does Fiedler use (fixed bytes/tick? RTT-derived? congestion-window?).
- How does he handle: new items, resends, acks, reliability guarantees?
- What edge cases does he explicitly call out (starvation, priority inversion, oscillation)?

### Application to Naia's four surfaces
- **Component updates:** does priority-per-component subsume the dirty-push model from Phase 3, or layer on top?
- **Entity commands (UnorderedReliable):** spawn-burst pacing. Does a large spawn batch get sliced across N ticks at budget B?
- **Plain messages:** when does a priority make sense vs FIFO? Games have chat, match events, control messages — do we need per-message-type priority or per-channel?
- **Request/response:** responses have implicit high priority (a client is blocked). How does this compose with the accumulator?

### Interaction with existing machinery
- Does the accumulator live in the per-peer outbound-packet loop (shared between server and client sides), or inside each channel sender? (Must be sender-side — both peers write outbound traffic; both need pacing.)
- How does it interact with RTT-based resend (today `ReliableSender::collect_messages`)?
- How does it interact with the dirty set from Phase 3 (dirty items = priority gains per tick)?
- How does it interact with MTU/packet-framing?
- Does it require protocol changes, or only sender-internal (symmetric on both peers)?

### Specific to the Phase 4.5 spike
- Does the accumulator, once applied to `ReliableSender`, eliminate the 17-tick resend burst?
- Concretely: at 10K queued spawns, RTT-triggered resend, budget B = MTU, does the accumulator split them into `ceil(10000 × msg_bytes / B)` ticks of uniform work?
- Does it still meet reliability guarantees (eventual delivery) under this pacing?

### Guardrails & failure modes
- How do we prevent starvation (low-priority items never sent)?
- How do we tune priority gain/drain without making it a footgun?
- What tests pin the behavior? (bandwidth budget respected; all items eventually sent; priority ordering observable in a repro).

---

## Deliverables

1. **Research doc** (`_AGENTS/PRIORITY_ACCUMULATOR_RESEARCH.md`): Fiedler's concept distilled from primary sources, prior art survey (other netcode frameworks), bandwidth math, explicit answers to the questions above.
2. **Implementation plan** (`_AGENTS/PRIORITY_ACCUMULATOR_PLAN.md`): phased rollout across the four surfaces, test strategy (unit + integration + namako specs + benches), risk register.
3. **Integration with BENCH_PERF_UPGRADE.md**: update phase table to reflect sidequest ordering (Sidequest → Phase 4.5 or merge → Phase 5).

Neither the implementation plan nor the code lands until Connor approves the research doc.

---

## Scope discipline

- **Do not implement during research.** Connor's rigor mandate. Research, plan, propose, wait for approval, then implement.
- **Do not scope-creep into protocol redesign.** Priority accumulator is a **sender-side pacing layer** (shared by server and client outbound paths); wire format stays Naia's current protocol unless research surfaces a hard blocker.
- **Do not ignore the test backstop.** `cargo test --workspace`, `namako gate` on all specs, `idle_distribution` and the bench matrix all stay green throughout.
- **Do not collapse the four-surface survey.** Even if one surface is "obviously" fine without it, say so explicitly in the research doc with evidence — don't just skip it.

---

## Handoff marker

If this sidequest completes cleanly, Phase 5 (spatial scope index) picks up with the priority accumulator already in place. Phase 5's design should then *assume* the accumulator as a primitive, not re-derive it. The plan doc's Phase 5 section will be updated accordingly as part of the sidequest close-out.
