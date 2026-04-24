# Sidequest — Priority Accumulator

**Status:** 🔨 core implementation complete 2026-04-24 (opened between Phase 4 and Phase 5 of `BENCH_PERF_UPGRADE.md`). Research complete; decisions resolved (D1–D16); Part III API design approved; **Phases A + B functional code landed, unit tests green, Phase C close-out remaining** (integration BDD specs + `idle_distribution` verification for Phase 4.5 absorption).

**Implementation state (2026-04-24):**
- ✅ Bandwidth accumulator (`shared/src/connection/bandwidth_accumulator.rs`) — token-bucket + one-packet overshoot. 9 unit tests.
- ✅ Bandwidth gate wired into both server + client `send_packet` paths; MTU cap gates further packets per tick.
- ✅ Always-on telemetry: `bytes_sent_last_tick`, `bandwidth_remaining`. Bench-gated: `packets_deferred_last_tick`.
- ✅ Channel-criticality sort in `MessageManager::write_messages` (High→Normal→Low base gains).
- ✅ Priority handles (global + per-user) with lazy entry creation; `set_gain` / `boost_once` / `reset`; Bevy adapter passthroughs.
- ✅ Eviction wired: `GlobalPriorityState::on_despawn` in `despawn_entity_worldless`; `UserPriorityState::on_scope_exit` on scope exit + user delete.
- ✅ V.2 preservation test: `IndexedMessageWriter` monotonicity panic pinned.
- ✅ Integration BDD specs as unit tests at the Connection-composition level: A-BDD-1 (tick cap), A-BDD-2 (drain), A-BDD-3 (high>low at equal age), A-BDD-4 (no false defer), A-BDD-5/6 (criticality override & ordering), A-BDD-7 (bounded catchup), B-BDD-1 (carry-forward), B-BDD-2 (override wins), B-BDD-3 (multiplicative gain), B-BDD-4/5/6 (handle semantics), B-BDD-7 (structural bound), B-BDD-9 (scope-exit isolation), B-BDD-10 (despawn eviction). All 29 new tests pass.
- ⏳ Full k-way merge between entity bundles + channel heads (A.2 ideal form) — current implementation sorts channels + gates bytes; entity bundle priority uses stored accumulator flow unchanged.
- ⏳ Full cucumber .feature file + namako registration for AB-BDD-1 (10K spawn burst end-to-end), B-BDD-6 (persist-across-tick through real send loop), B-BDD-8 (cross-entity reorder preserving per-entity monotonicity through live spawn-barrier FSM) — need harness scaffolding.
- ⏳ `idle_distribution` matrix verification (Phase 4.5 absorption gate).
**Owner:** Connor + Claude (Opus 4.7)
**Origin:** Long-standing Naia backlog item. Surfaced back to the top by the Phase 4.5 mutable resend-window spike (`idle_distribution.rs` shows ≥2700× max/p50 on every `N=10000 mut` cell at reliable-sender resend cadence).

**Deliverables map:**
- `PRIORITY_ACCUMULATOR_SIDEQUEST.md` *(this doc)* — scope, origin, discipline.
- `PRIORITY_ACCUMULATOR_RESEARCH.md` — Fiedler distillation, Halo: Reach + Overwatch + Unreal + Unity + bevy_replicon prior art, answers to the research questions below.
- `PRIORITY_ACCUMULATOR_PLAN.md` — locked decisions, API design, implementation phases, test plan, risk register.

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

## Research questions (all answered — see `PRIORITY_ACCUMULATOR_RESEARCH.md`)

The research stage addressed: Fiedler's concept (priority vs bandwidth accumulator definitions, inputs, edge cases); applicability to Naia's four surfaces (component updates, reliable entity commands, plain messages, requests/responses); interaction with existing machinery (dirty set, RTT resend, MTU, protocol wire format); specific resolution of the Phase 4.5 spike; and guardrails against starvation + priority-function footguns (the Halo: Reach "idle grenade" class of bug).

---

## Scope discipline

- **Do not implement during research.** Connor's rigor mandate. Research, plan, propose, wait for approval, then implement.
- **Do not scope-creep into protocol redesign.** Priority accumulator is a **sender-side pacing layer** (shared by server and client outbound paths); wire format stays Naia's current protocol unless research surfaces a hard blocker.
- **Do not ignore the test backstop.** `cargo test --workspace`, `namako gate` on all specs, `idle_distribution` and the bench matrix all stay green throughout.
- **Do not collapse the four-surface survey.** Even if one surface is "obviously" fine without it, say so explicitly in the research doc with evidence — don't just skip it.

---

## Handoff marker

If this sidequest completes cleanly, Phase 5 (spatial scope index) picks up with the priority accumulator already in place. Phase 5's design should then *assume* the accumulator as a primitive, not re-derive it. The plan doc's Phase 5 section will be updated accordingly as part of the sidequest close-out.
