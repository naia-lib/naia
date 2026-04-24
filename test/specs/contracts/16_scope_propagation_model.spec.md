# Contract 16 — Scope Propagation Model

**Phase:** 2 (Push-based scope-change tracking)
**Status:** Active

## Purpose

Specifies the behavioral invariants that the push-based scope-change queue (Win 2)
must preserve relative to the prior per-tick full-scan implementation.  These
obligations are regression guards: every one of them is observable before the
refactor.  If any fails under the new code path (`v2_push_pipeline` feature flag),
the refactor has broken something.

## Vocabulary

- **scope-change API call**: any of `UserScope::include`, `UserScope::exclude`,
  `room_add_user`, `room_remove_user`, `room_add_entity`, `room_remove_entity`.
- **idle tick**: a tick in which no scope-change API call was made and no entity
  was spawned or despawned since the previous tick.
- **scope_change_queue depth**: the number of unprocessed entries in
  `WorldServer::scope_change_queue` (exposed via the test-utils accessor
  `Scenario::scope_change_queue_len()`; returns 0 for the legacy scan path).

---

## Obligations

### scope-propagation-01 (t1)

> After any sequence of scope-change API calls, the set of entities the client
> observes must be identical to what the pre-refactor eager-scan path produced.

Specifically:
- A user added to a room MUST see all current room entities by the next tick.
- An entity added to a room MUST become visible to all current room users by the next tick.
- A `UserScope::include(E)` MUST add E to the client's entity set by the next tick.
- A `UserScope::exclude(E)` MUST remove E from the client's entity set by the next tick.
- All of the above hold even when multiple changes are applied within the same tick.

### scope-propagation-02 (t2)

> During an idle tick (no scope-change API calls), `scope_change_queue` depth
> MUST be 0 after the tick is processed.

Intent: verifies that the queue drains to empty and no work is done when nothing
has changed.  Applies both to the legacy path (trivially 0 — no queue exists) and
to the push-based path (queue drains every tick).

### scope-propagation-03 (t3)

> A scope-change API call enqueued during tick N MUST be fully applied by the end
> of tick N.  The `scope_change_queue` depth after `server.update()` runs MUST be 0.

This is the latency guarantee: scope changes are not deferred by more than one tick.

### scope-propagation-04 (t4)

> Calling `UserScope::include` or `UserScope::exclude` for an entity that does not
> exist in the server's global entity registry MUST be a silent no-op.  The server
> MUST NOT panic, MUST NOT corrupt any existing scope state, and MUST continue
> running normally.
