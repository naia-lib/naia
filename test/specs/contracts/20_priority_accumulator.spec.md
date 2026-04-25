# Contract 20 — Priority Accumulator (Sidequest)

## Context

The Priority Accumulator sidequest (PRIORITY_ACCUMULATOR_PLAN.md) adds a
two-layer per-entity priority system (`global × per_user`) and a token-bucket
bandwidth accumulator that together cap outbound bytes-per-tick and arbitrate
between pending entity-bundles and per-channel FIFOs. The plan's Part V.3
enumerates a set of BDD obligations; most are covered at the unit/integration
level in `shared/src/connection/priority_accumulator_integration_tests.rs`.

This contract captures the three obligations that require a full server +
client round-trip to exercise meaningfully:

- **AB-BDD-1** — Spawn-burst + bandwidth pressure: under bandwidth constraints,
  a batch of freshly-spawned in-scope entities eventually reaches the client
  even when a single tick cannot carry them all.
- **B-BDD-6** — Gain persistence across send: `set_gain(N)` on the global
  layer persists across the send cycle; subsequent reads still return `Some(N)`.
- **B-BDD-8** — Cross-entity reorder under pressure: when the priority sort
  reorders bundles across different entities within a tick, each individual
  entity's observable component-value sequence remains consistent (the client
  converges to the latest server value — the invariant CommandId monotonicity
  guarantees at the wire level).

---

## Obligations

### t1 — Spawn-burst drains under budget (AB-BDD-1)

A server spawns many entities in one tick, all in-scope for a single client.
Even if a single packet's payload cannot carry every bundle, every spawned
entity eventually appears on the client. No entity is starved by repeatedly
losing the priority sort to later spawns.

Expressed observationally: after N ticks of forward progress (where N is well
bounded by `ceil(N_entities × bytes_per_entity / budget_per_tick)`), the
client's world contains all spawned entities with their replicated component
values intact.

### t2 — Gain persistence across send (B-BDD-6)

After `server.set_global_entity_gain(entity, 5.0)`:
- `server.global_entity_gain(entity)` returns `Some(5.0)`.
- After driving enough ticks for the entity to be fully replicated to the
  client, `server.global_entity_gain(entity)` still returns `Some(5.0)`.

The accumulator resets to 0 on send; the `gain_override` does not.

### t3 — Per-entity value convergence under cross-entity reorder (B-BDD-8)

Two entities A and B are spawned and in-scope for a single client. The server
mutates A's component in tick T, then mutates B's component in tick T+1, then
mutates A's component again in tick T+2. Under priority sort, bundles across A
and B may be reordered within any given send cycle, but per-entity CommandId
monotonicity is preserved at the wire level. Observable invariant:

- Client eventually observes A with its final (tick T+2) value.
- Client eventually observes B with its final (tick T+1) value.
- Client never observes A with an intermediate-but-stale final state.

---

## Cross-references

- Unit coverage for the data types: `shared/src/connection/entity_priority.rs`
  (tests `b_bdd_6_set_gain_persists_across_mutations`,
  `b_bdd_5_boost_once_does_not_mutate_gain`, etc.).
- Integration coverage for the full loop:
  `shared/src/connection/priority_accumulator_integration_tests.rs`
  (`a_bdd_*`, `b_bdd_*`).
- Plan source: `_AGENTS/PRIORITY_ACCUMULATOR_PLAN.md` Part V.3.
