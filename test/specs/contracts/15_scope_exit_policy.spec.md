# Scope Exit Policy — `ScopeExit::Persist`

Phase 1 introduces `ScopeExit` as a field on `ReplicationConfig`.  The default value,
`ScopeExit::Despawn`, preserves all existing scope-exit semantics (entity leaving scope
is despawned on the client's side).  The new variant, `ScopeExit::Persist`, changes the
per-user scope-exit effect: instead of emitting a Despawn, the server pauses replication
for that `(user, entity)` pair and retains the entity in the client's networked entity pool.

This spec does not redefine:
- Scope membership predicate or room gating (see `06_entity_scopes.spec.md`)
- Publicity (Private / Public / Delegated) — unchanged by this phase
- Authority or delegation semantics

---

## 1) Vocabulary

- **ScopeExit::Despawn** (default): when `OutOfScope(U,E)` is resolved, emit a `Despawn`
  command to user `U`'s connection, removing `E` from the client's networked entity pool.
- **ScopeExit::Persist**: when `OutOfScope(U,E)` is resolved, **pause** replication for
  `(U,E)` instead of emitting a `Despawn`. The entity stays in the client's networked
  entity pool; no further update messages are sent while paused.
- **Paused(U,E)**: the server-side state for a `(user, entity)` pair where `ScopeExit::Persist`
  is configured and the entity is currently `OutOfScope(U,E)`.
- **Resumed(U,E)**: when a `Paused(U,E)` pair transitions back to `InScope(U,E)`, the
  server resumes replication and emits any accumulated mutations.

---

## 2) Backward Compatibility — Default is Despawn

### [scope-exit-01] — Default ScopeExit preserves existing Despawn semantics

**Obligations:**
- **t1**: An entity configured with `ReplicationConfig::public()` (no explicit `ScopeExit`)
  MUST behave identically to the prior `ReplicationConfig::Public` variant: when the entity
  leaves a user's scope, the server MUST emit a Despawn to that user's client.

This obligation exists to confirm the refactor is backward-compatible.  No existing scenario
in contracts 06–14 may regress due to this phase.

---

## 3) Persist — Scope Exit Behavior

### [scope-exit-02] — Persist entity leaving scope is not despawned on client

**Obligations:**
- **t1**: When `ScopeExit::Persist` is configured and `OutOfScope(U,E)` is resolved, the
  server MUST NOT emit a Despawn command for `E` to user `U`'s connection.  `E` MUST
  remain present in the client's networked entity pool (i.e. `client.has_entity(E)` is
  `true`).

### [scope-exit-03] — Persist entity out-of-scope receives no replication updates

**Obligations:**
- **t1**: While `Paused(U,E)`, the server MUST NOT forward any component update messages
  for `E` to user `U`.  The client's component state for `E` is frozen at the snapshot
  from the moment scope was lost.

### [scope-exit-04] — Persist entity re-entering scope resumes with accumulated deltas

**Obligations:**
- **t1**: When `InScope(U,E)` is resolved after a period of `Paused(U,E)`, the server
  MUST resume replication and deliver accumulated component mutations that occurred during
  the paused period.  The client MUST eventually observe the authoritative server state.
- **t2**: When `InScope(U,E)` is resolved after a period of `Paused(U,E)` with **no**
  server-side mutations during the absence, the entity MUST remain present on the client
  with no new spawn event and no spurious update bytes emitted.

---

## 4) Persist — Global Despawn During Absence

### [scope-exit-05] — Persist entity globally despawned while out-of-scope propagates to client

**Obligations:**
- **t1**: If the server globally despawns entity `E` while `Paused(U,E)` holds, the server
  MUST eventually deliver a Despawn command for `E` to user `U`.  The client MUST remove
  `E` from its networked entity pool.

---

## 5) Persist — Component Lifecycle During Absence

### [scope-exit-06] — Component inserted during Persist absence becomes visible on re-entry

**Obligations:**
- **t1**: When a component `C` is inserted on entity `E` while `Paused(U,E)` holds, user
  `U`'s client MUST observe `C` on `E` after re-entry (`E` is back `InScope(U,E)`).

### [scope-exit-07] — Component removed during Persist absence is absent on re-entry

**Obligations:**
- **t1**: When a component `C` is removed from entity `E` while `Paused(U,E)` holds, user
  `U`'s client MUST NOT have `C` on `E` after re-entry.

---

## 6) Persist — Disconnect Cleanup

### [scope-exit-08] — Disconnect while entity is Paused cleans up without panic

**Obligations:**
- **t1**: When user `U` disconnects while `Paused(U,E)` holds for any entity `E`, the
  server MUST release all per-user state (including the paused entity record) without
  panicking.  Post-disconnect, the server MUST NOT hold references to `U`'s connection.

---

## 7) State Transition Table: ScopeExit::Persist

| Current State         | Trigger                              | Next State            | Client Effect                         |
|-----------------------|--------------------------------------|-----------------------|---------------------------------------|
| `InScope(U,E)`        | scope exit, `ScopeExit::Persist`     | `Paused(U,E)`         | no Despawn; replication frozen        |
| `InScope(U,E)`        | scope exit, `ScopeExit::Despawn`     | `OutOfScope(U,E)`     | Despawn emitted (existing behavior)   |
| `Paused(U,E)`         | scope re-entry                       | `InScope(U,E)`        | accumulated deltas forwarded          |
| `Paused(U,E)`         | global server despawn of `E`         | `(removed)`           | Despawn emitted to U's client         |
| `Paused(U,E)`         | user `U` disconnects                 | `(removed)`           | per-user state cleaned up             |
| `Paused(U,E)`         | component inserted on `E`            | `Paused(U,E)`         | deferred; emitted on re-entry         |
| `Paused(U,E)`         | component removed from `E`           | `Paused(U,E)`         | deferred; applied on re-entry         |
