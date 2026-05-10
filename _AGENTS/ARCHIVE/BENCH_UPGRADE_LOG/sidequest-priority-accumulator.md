# Sidequest — Priority Accumulator (CLOSED 2026-04-24)

**Outcome:** ✅ Complete. Glenn Fiedler's priority accumulator + token-bucket bandwidth accumulator is now the canonical sender-side pacing layer in Naia, applied symmetrically on both server and client outbound paths. Phase 4.5 (mutable resend-window spike) closed by absorption — same `idle_distribution` matrix that previously showed 2741–4033× max/p50 on `*_10000e_mut` cells now reports `OK` (≤6.6×) across the board.

## Phases

| Phase | Description | State |
|-------|-------------|-------|
| Research | Fiedler distillation, four-surface survey, prior-art (Halo: Reach, Overwatch, Unreal, Unity, bevy_replicon) | ✅ |
| Planning | API design, locked decisions D1–D16, risk register | ✅ |
| Stage 1 | Stub all public APIs with `todo!()` for cross-cutting compile gate | ✅ |
| Phase A | Bandwidth accumulator (token-bucket + one-packet overshoot), wired into both `send_packet` paths, channel-criticality sort | ✅ |
| Phase B | Two-layer priority handles (`global × per_user`), eviction wiring (`despawn_entity_worldless` + scope-exit + user-delete), V.2 monotonicity preservation | ✅ |
| Phase C.1 | V.4 gate sweep (cargo test + namako + criterion) | ✅ |
| Phase C.2 | **Full k-way merge (A.2 ideal form)** — `OutgoingPriorityHook` trait, per-tick advance/sort/reset orchestration, sorted entity-bundle iteration in `WorldWriter::write_updates` | ✅ |
| Phase C.3 | Cucumber `.feature` specs for AB-BDD-1, B-BDD-6, B-BDD-8 | ✅ |
| Phase C.4 | Sidequest log + close-out (this file) | ✅ |
| Phase C.5 | Pre-existing demo build errors fixed during sweep | ✅ |
| Phase 4.5 | Absorbed by Phase A bandwidth cap | ✅ (closed) |

## Architecture summary

The pacing layer is split across two disjoint accumulators:

1. **Bandwidth accumulator** (`shared/src/connection/bandwidth_accumulator.rs`):
   token-bucket with one-packet overshoot, ticked at the start of each
   `Connection::send_packets`. Gates further packets per tick; unsent items
   compound into the next tick. Symmetric on server and client.

2. **Priority accumulator** (`shared/src/connection/priority_state.rs` +
   `entity_priority.rs`): two-layer (`GlobalPriorityState<E>` ×
   `UserPriorityState<E>`), keyed per-entity. The send loop calls
   `OutgoingPriorityHook::advance(entity)` to add `effective_gain` (defaults
   1.0×1.0) into the accumulator, then sorts the dirty-bundle list
   descending. After the packet loop, drained bundles get
   `reset_after_send(entity, tick)`. The hook is keyed by `GlobalEntity` so
   the trait stays in `naia-shared`; the `WorldServer`-side adapter
   (`WorldServerPriorityHook`) bridges to the `E`-typed state via the
   `GlobalEntityMap`.

3. **K-way merge** is intra-section: `WorldWriter::write_updates` accepts
   `Option<&[GlobalEntity]>` and consumes the priority-sorted Vec to emit
   highest-priority entity bundles first within the updates section. Wire
   format unchanged. Cross-section fairness is gated by the bandwidth
   budget; carry-forward across ticks recomputes priorities, so persistent
   high-priority entities re-win, persistent low-priority entities are
   eligible after enough accumulator ticks.

4. **Eviction hooks** are wired into the canonical lifecycle paths —
   `WorldServer::despawn_entity_worldless` calls `global_priority.on_despawn`
   and `user_priorities.values_mut().on_scope_exit`; user delete drops the
   whole `user_priorities` entry; scope exit drops the per-user entry.

## Test coverage

| Layer | Test surface | Count |
|-------|--------------|-------|
| Unit (data types) | `entity_priority.rs` | b_bdd_1..6 + handle semantics |
| Unit (state) | `priority_state.rs` | despawn / scope-exit eviction |
| Integration | `priority_accumulator_integration_tests.rs` | A-BDD-1..7, B-BDD-1..10 (29 tests) |
| Cucumber (round-trip) | `test/specs/features/20_priority_accumulator.feature` | AB-BDD-1, B-BDD-6, B-BDD-8 |
| Wire format | `IndexedMessageWriter` monotonicity panic | preservation pinned |

`cargo test --workspace`: 171 lib tests + 29 integration tests pass.
`namako gate`: lint + run + verify all PASS on full corpus.

## Lessons

1. **Sender-side, not server-side.** First-pass framing kept slipping into
   "server priority" — but Naia is sender-side symmetric. Client-authoritative
   entities/messages/requests use the exact same outbound machinery.
   The eviction hooks, the bandwidth accumulator, and the priority handles
   all live in `naia-shared` and are called from both connection types.

2. **Wire format constraint dictated intra-section sort, not byte-level
   interleave.** `write_into_packet` writes messages → updates → commands
   in fixed order. The k-way merge applies *within* the updates section
   (sort entity bundles by priority); cross-section competition is bandwidth-
   budget gated. This is the right call: any byte-level interleave would
   require a wire-format break for marginal benefit.

3. **Hook keyed by `GlobalEntity`, adapter on the server side.** The trait
   stays in `naia-shared` (`OutgoingPriorityHook` keyed by `GlobalEntity`),
   the `E`-typed state lives in `WorldServer`, and a thin adapter
   (`WorldServerPriorityHook`) bridges them via the `GlobalEntityMap`.
   Disjoint-field split-borrow on `WorldServer` (global_priority +
   user_priorities[user_key] + global_entity_map) constructs the adapter
   per-connection without lifetime gymnastics.

4. **Cucumber-expressions parens trap.** `(x, y)` in step text is an
   optional group, not a literal. Use `x=10 y=20` matched by `{int}`
   instead. Cost a regenerate cycle to discover.

5. **Phase 4.5 absorption was real, not aspirational.** Pre-sidequest:
   `1u_10000e_mut` max/p50 = 3007×. Post-sidequest: 5.8×. The token bucket
   spreads the 10K-item resend burst across ticks instead of letting it
   spike on the resend-window cadence. No dedicated Phase 4.5 fix was
   needed.

## Files touched (non-exhaustive)

- `shared/src/connection/bandwidth_accumulator.rs` (new)
- `shared/src/connection/priority_state.rs` (new — `OutgoingPriorityHook`,
  `NoopPriorityHook`, `GlobalPriorityState`, `UserPriorityState`)
- `shared/src/connection/entity_priority.rs` (new — handles)
- `shared/src/connection/priority_accumulator_integration_tests.rs` (new)
- `shared/src/world/world_writer.rs` (entity_priority_order param)
- `shared/src/connection/base_connection.rs` (entity_priority_order param)
- `shared/src/messages/message_manager.rs` (channel-criticality sort)
- `server/src/connection/connection.rs` (advance/sort/reset orchestration,
  hook param)
- `server/src/server/world_server.rs` (`WorldServerPriorityHook` adapter,
  per-connection construction)
- `client/src/connection/connection.rs` (mirrored signature, `None` priority
  passed — single connection, no arbitration)
- `test/specs/features/20_priority_accumulator.feature` (new)
- `test/specs/contracts/20_priority_accumulator.spec.md` (new)
- `test/tests/src/steps/priority_accumulator.rs` (new)

## Handoff

Phase 5 (spatial scope index) is unblocked. Phase 5's design should *assume*
the priority accumulator as a primitive, not re-derive it. Spatial scope
candidates that survive culling can flow directly through the per-entity
priority advance — distance-from-camera or LOD bucket becomes a natural
input to a future `set_gain` policy.
