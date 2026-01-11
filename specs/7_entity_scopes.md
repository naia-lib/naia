# Entity Scopes

Entity Scopes define whether a given Entity `E` is **in-scope** or **out-of-scope** for a given User/Client `U`,
and the required observable consequences of scope transitions.

This spec defines:
- The **scope membership predicate** (Rooms + per-user include/exclude filters + required coupling).
- The **state machine** for `InScope(U,E)` / `OutOfScope(U,E)` and its client-visible effects.
- Deterministic **tick-level collapse** rules for scope changes.
- Required behavior under reordering / illegal states.

This spec does not define:
- Ownership write permissions (see `9_entity_ownership.md`)
- Publication gating for client-owned entities (see `10_entity_publication.md`)
- Delegation/authority semantics (see `11_entity_delegation.md`, `12_entity_authority.md`)
- Replication ordering/wire format (see `8_entity_replication.md`)

---

## 1) Vocabulary

- **User U**: a server-identified remote client/user (keyed by `user_key`).
- **Entity E**: a networked entity tracked by Naia replication.
- **Room**: a server-managed grouping for coarse scope gating (users and entities may be members of multiple rooms).
- **SharesRoom(U,E)**: true iff `U` and `E` share at least one common room.
- **Include(U,E)**: per-user scope inclusion filter set via `server.user_scope_mut(user_key).include(entity)`.
- **Exclude(U,E)**: per-user scope exclusion filter set via `server.user_scope_mut(user_key).exclude(entity)`.

### Diagnostics
- **Diagnostics enabled**: a build/feature/runtime mode where Naia may emit warnings for illegal/impossible states.
  When diagnostics are not enabled (production default), Naia must remain silent.

---

## 2) Core Scope Predicate

### entity-scopes-01 — Rooms are a required coarse gate for non-owners
For any user `U` and entity `E`, `SharesRoom(U,E)` MUST be a necessary precondition for `InScope(U,E)`, except where
other specs explicitly override (e.g. owning client always in-scope for its client-owned entities; see below).

If `SharesRoom(U,E) == false`, then `OutOfScope(U,E)` MUST hold.

### entity-scopes-02 — Per-user include/exclude is an additional filter (additive after Rooms)
Assuming `SharesRoom(U,E) == true`, the server MUST apply the per-user filter as follows:

- If `Exclude(U,E)` is active, then `OutOfScope(U,E)` MUST hold.
- Else if `Include(U,E)` is active, then `InScope(U,E)` MUST hold (subject to other gates like publication).
- Else (neither active), the default MUST be `InScope(U,E)` (subject to other gates like publication).

### entity-scopes-03 — Include/Exclude ordering: last call wins
If both `Include(U,E)` and `Exclude(U,E)` are applied over time, the effective filter state MUST be determined by
the most recently applied call for that `(U,E)` pair (last call wins).

This rule is defined in terms of the server’s resolved mutation order (i.e. “last call” means last in the server’s
finalized application order for that tick).

### entity-scopes-04 — Roomless entities are out-of-scope for all non-owners
If `E` is in zero rooms, then for all users `U` that are not explicitly forced in-scope by other specs,
`OutOfScope(U,E)` MUST hold, regardless of `Include(U,E)`.

(Include/exclude does not bypass the Rooms gate.)

---

## 3) Required Coupling to Ownership & Publication

### entity-scopes-05 — Owning client is always in-scope for its client-owned entities
For a client-owned entity `E` with owning client `A`:
- `InScope(A,E)` MUST always hold while `A` is connected.
- Publication and per-user scope filters MUST NOT remove `E` from `A`’s scope.

(This restates the required coupling from `9_entity_ownership.md` / `10_entity_publication.md` as a scope invariant.)

### entity-scopes-06 — Publication can force non-owners out-of-scope
For client-owned entities, publication state MUST be treated as an additional gate for non-owners:
- If client-owned `E` is Unpublished/Private, then for all `U != Owner(E)`, `OutOfScope(U,E)` MUST hold.

(See `10_entity_publication.md` for publication semantics; this spec defines the scope consequence.)

---

## 4) Scope State Machine & Client-Visible Effects

For each pair `(U,E)` from the server’s perspective, the scope state is exactly one of:
- `InScope(U,E)`
- `OutOfScope(U,E)`

### entity-scopes-07 — OutOfScope ⇒ despawn on that client
When a client corresponding to user `U` becomes `OutOfScope(U,E)`:
- `E` MUST be despawned on that client (removed from the client’s networked entity pool).

### entity-scopes-08 — Despawn destroys all components, including local-only components
When `E` despawns on a client due to leaving scope:
- all components associated with `E` in that client’s networked entity pool MUST be destroyed,
  including any local-only components the client may have attached.

### entity-scopes-09 — OutOfScope ⇒ ignore late replication updates for that entity
If a client receives replication updates for an entity `E` that is currently `OutOfScope` on that client:
- the client MUST ignore them silently in production.
- when diagnostics are enabled, the client MAY emit a warning.

This rule exists to make the protocol tolerant to packet reordering and racey delivery.

### entity-scopes-10 — InScope ⇒ entity exists in networked entity pool
If a client is `InScope(U,E)`, then `E` MUST exist in that client’s networked entity pool (i.e. be present as a
replicated/spawned entity), subject to normal replication delivery and eventual consistency.

---

## 5) Tick Semantics & Collapse Rules

### entity-scopes-11 — Scope is resolved per server tick; intermediate states are not observable
The server MUST resolve the final scope state for each `(U,E)` once per server tick and emit only the delta from
the prior tick’s resolved state.

If within one server tick operations would cause `InScope(U,E)` to flip multiple times (e.g. add/remove room membership,
include/exclude toggles), the server MUST collapse to the final resolved state and MUST NOT emit intermediate
spawn/despawn transitions.

### entity-scopes-12 — Leaving scope for ≥1 tick creates a new lifetime on re-entry
If a client transitions `InScope(U,E) → OutOfScope(U,E)` and remains OutOfScope for at least one full server tick,
then the next transition `OutOfScope(U,E) → InScope(U,E)` MUST be treated by the client as a **fresh spawn lifetime**:
- the entity MUST spawn as if new,
- the client MUST NOT rely on any prior lifetime’s state,
- the server MUST provide an authoritative snapshot baseline for the new lifetime consistent with replication rules.

If the entity leaves and re-enters within the same tick and the server collapses to “still InScope,” then no lifetime
boundary occurs (no observable spawn/despawn).

---

## 6) Disconnect Handling

### entity-scopes-13 — Disconnect implies OutOfScope for that user for all entities
When a client disconnects (user `U` removed from the server connection set):
- `OutOfScope(U,E)` MUST be treated as holding for all entities `E` immediately.
- The server MUST cease replicating entities to that client.

Note: Separately, `9_entity_ownership.md` defines that client-owned entities are globally despawned when their owning
client disconnects. This spec does not redefine that rule; it defines per-user scope state.

---

## 7) Illegal / Misuse Cases

These cases SHOULD NOT occur in correct usage, but behavior is defined for determinism and safety.

### entity-scopes-14 — Include/exclude without shared room cannot force scope
If `Include(U,E)` is active but `SharesRoom(U,E) == false`, then `OutOfScope(U,E)` MUST hold.

When diagnostics are enabled, the server MAY emit a warning indicating the include is ineffective due to room gating.

### entity-scopes-15 — Unknown entity/user references
If the server receives (or internally attempts) a scope operation referencing an unknown entity or unknown user:
- in production, it MUST ignore the operation silently.
- when diagnostics are enabled, it MAY emit a warning.

---

## 8) Test obligations (TODO placeholders; not implementing yet)

- **entity-scopes-01/04**: Prove Rooms gating is necessary; roomless entities out-of-scope for non-owners.
- **entity-scopes-02/03**: Prove include/exclude filter works, last call wins, and does not bypass Rooms gate.
- **entity-scopes-05**: Prove owning client always in-scope for its client-owned entities while connected.
- **entity-scopes-06**: Prove Private/Unpublished forces OutOfScope for all non-owners.
- **entity-scopes-07/08**: Prove leaving scope despawns and destroys all components including local-only.
- **entity-scopes-09**: Prove late updates for out-of-scope entities are ignored (warn only when diagnostics enabled).
- **entity-scopes-11**: Prove same-tick flip-flops collapse to final state; no intermediate spawn/despawn.
- **entity-scopes-12**: Prove re-entry after ≥1 tick out-of-scope produces fresh spawn snapshot lifetime.
- **entity-scopes-13**: Prove disconnect implies OutOfScope for that user and replication ceases.

---

## 9) Cross-references

- Ownership: `9_entity_ownership.md`
- Publication: `10_entity_publication.md`
- Replication ordering/wire rules: `8_entity_replication.md`
- Delegation/Authority coupling: `11_entity_delegation.md`, `12_entity_authority.md`
- Events/lifetimes: `13_server_events_api.md`, `14_client_events_api.md`, `15_world_integration.md`
