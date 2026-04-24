# Contract 17 — Update Candidate Set Model (Phase 3)

## Context

`HostWorldManager::get_updatable_world` currently builds a fresh
`HashMap<GlobalEntity, HashSet<ComponentKind>>` every tick by iterating every
host-world entity channel, regardless of whether anything has mutated.
`EntityUpdateManager::take_outgoing_events` then filters that map by
`diff_mask_is_clear`. For 10K mostly-idle entities this builds and
mostly-discards a 10K-entry map every tick.

Phase 3 replaces the full scan with a per-connection dirty set populated at
mutation time. `MutChannelData::send` pushes `(GlobalEntity, ComponentKind)`
into each scoped user's dirty-candidate set when a property changes.
`take_update_events` drains that set instead of scanning all host entities.

Idle-tick cost: O(1) — empty dirty set, no iteration.
Mutation-tick cost: O(mutations) — only affected pairs are evaluated.

---

## Obligations

### t1 — Behavioral equivalence

Component mutations produce identical client-observable updates under Phase 3
as they did under the full-scan path. No change to wire semantics.

Covered by: existing Contract 7 (entity_replication) scenarios.

### t2 — Idle entity produces no dirty candidates

After a tick in which no component mutations occur and no scope changes happen,
the total dirty update candidate count across all user connections is 0.

On the legacy path: no dirty set exists; returns 0.
On the Phase 3 path: dirty set is empty because no mutations fired; returns 0.

### t3 — Mutation candidate drains in tick

After a component property is mutated, the ensuing tick drains the dirty
candidate set to 0, and the client observes the update.

On the legacy path: returns 0 (no dirty set); update still reaches client.
On the Phase 3 path: set is populated at mutation time, drained to 0 in the
same tick; update reaches client.

### t4 — Out-of-scope mutation produces no candidate

When a component is mutated on an entity that is not in a shared room with
any user, no dirty candidate is enqueued for any connection.

On the legacy path: returns 0 always.
On the Phase 3 path: `MutChannelData.receiver_map` has no entry for the
out-of-scope user's address, so no dirty notification fires.
