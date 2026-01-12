# Entity Replication

This spec defines the **client-observable behavior** of Naia’s entity/component replication over the wire:
- entity spawn/despawn as perceived by a client
- replicated component insert/update/remove ordering
- tolerance to packet **reordering**, **duplication**, and **late arrival**
- entity identity across **lifetimes** (scope enter → scope leave, with the ≥1 tick rule)

This spec does **not** define:
- RPC/message semantics (see `3_messaging.md`)
- the internal serialization format
- bandwidth/compression strategies

---

## Glossary

- **Replicated component**: a component type that is part of the Protocol and may be synced over the wire.
- **Local-only component**: a component instance present only in a local World that is not (currently) server-replicated for that entity.
- **Entity lifetime (client-side)**: `scope enter → scope leave`, where re-entering scope after being out-of-scope for **≥ 1 tick** is a **new lifetime** (fresh spawn semantics). See `6_entity_scopes.md`.
- **GlobalEntity**: global identity of an entity across the server’s lifetime (monotonically increasing u64; practical uniqueness).
- **LocalEntity (HostEntity/RemoteEntity)**: per-connection entity handle(s) that may wrap/reuse across lifetimes; must be disambiguated by lifetime rules.

---

### Entity lifetime (client)

For a given client, an entity lifetime is:
`scope enter` → `scope leave`, with the rule that re-entering scope after ≥ 1 tick out-of-scope is a fresh lifetime.

Normative:
- Entity-specific replicated writes (insert/remove/update) MUST be ignored if they refer to an entity outside its current lifetime.
- If an Update arrives before its corresponding Insert due to packet reordering, the Update MUST be buffered until the Insert arrives (or discarded if the lifetime ends first).


## Contract

### [entity-replication-01] — Global identity stability
While an entity exists on the server:
- The entity MUST have a stable **GlobalEntity**.
- The server MUST NOT change an entity’s GlobalEntity during its existence.

When the server despawns the entity:
- That entity ceases to exist. Any future entity with a different lifetime is a different entity, even if some local IDs are reused.

---

### [entity-replication-02] — Client-visible lifetime boundaries
For any given client `C` and entity `E`, Naia MUST model a client-visible **lifetime**:

- Lifetime **begins** when `E` enters `C`’s scope and Naia emits a **Spawn** to `C`.
- Lifetime **ends** when `E` leaves `C`’s scope (including unpublish) and Naia emits a **Despawn** to `C`.
- If `E` re-enters scope after being out-of-scope for **≥ 1 tick**, Naia MUST treat this as a **new lifetime** with **fresh spawn snapshot semantics**.

Cross-link:
- Scope/lifetime rules are defined in `6_entity_scopes.md` and are binding here.

---

### [entity-replication-03] — Spawn snapshot semantics (baseline state)
When `E` enters scope for client `C`, the Spawn sent to `C` MUST include:

- The set of replicated components present on `E` **at the time the Spawn is sent**
- For each included replicated component, the full replicated field state necessary to establish the baseline

Client-side requirement:
- The client MUST be able to materialize the entity’s replicated baseline solely from the Spawn snapshot.

Non-normative note:
- This allows replication to avoid requiring “insert-before-update” for initial state; Spawn is the baseline.

---

### [entity-replication-04] — No observable replication before Spawn
For a given client-visible lifetime of `(C, E)`:

- The client MUST NOT observe any replicated component Insert/Update/Remove for `E` **before** it observes the Spawn for that lifetime.
- If delivery order causes the client to receive component actions before Spawn, Naia MUST ensure those actions are **not observable early** (either by buffering or by deferring application until Spawn becomes available).

This is a hard invariant: **no update-before-spawn** observability.

---

### [entity-replication-05] — Actions outside lifetime are ignored
If the client receives any entity/component replication action referencing an entity lifetime that is not currently active (i.e. before Spawn for that lifetime, or after Despawn for that lifetime):

- Naia MUST ignore the action (it MUST NOT mutate world state).
- In production, this MUST be silent.
- In Debug mode, Naia MAY emit a warning.

This applies to:
- late packets from a prior lifetime
- reordered packets that arrive after the lifetime ended
- packets referencing entities that are out-of-scope

---

### [entity-replication-06] — Update-before-Insert buffering (within lifetime)
Within an active lifetime:

- If a replicated component **Update** is received before the corresponding replicated component **Insert** has been applied, Naia MUST buffer the Update and apply it after Insert arrives.
- Buffered updates MUST be dropped when the lifetime ends (on Despawn), if they have not been applied.
- Naia MUST NOT apply a buffered Update to an entity/component that belongs to a different lifetime.

The same rule applies symmetrically for any component action that requires the component to exist first (e.g. Remove received before Insert): Naia MUST ensure the action is not misapplied.

---

### [entity-replication-07] — Local-only component overwrite by server replication
If, at the time a replicated component Insert (or Spawn snapshot) is applied, the client already has a **local-only** component instance of the same component type on that entity:

- This overwrite MUST be surfaced as an Insert (replicated-backed component becomes present), even though a local-only instance existed.
- Naia MUST treat the replicated state as authoritative going forward.

Observability rule:
- If a local-only component existed and is overwritten by an incoming server-replicated component Insert (or Spawn snapshot),
  Naia MUST emit a client-visible **Insert** event for that component (presence becomes “replicated-backed”),
  not an Update event.

Cross-link:
- Ownership rules for local-only components vs server-backed replicated components are defined in `8_entity_ownership.md`. This contract ensures replication behavior conforms.

---

### [entity-replication-08] — Collapse to final state per tick (no intermediate transitions)
Within a single server tick, if an entity/component undergoes multiple changes that would otherwise create intermediate states (insert+remove, multiple updates, etc.):

- The server MUST collapse replication to the **final state** for that tick.
- The client MUST NOT be forced to observe intermediate states that did not persist across ticks.

This mirrors the “final state only” principle used in scope transitions.

---

### [entity-replication-09] — Duplicate delivery is idempotent
If the client receives duplicate replication actions (e.g. due to retransmission):

- Applying the same logical action more than once MUST NOT create additional observable effects.
- Naia MUST remain convergent to the server’s final replicated state.

Examples (normative intent):
- duplicate Spawn for an already-spawned active lifetime MUST NOT create a second entity
- duplicate Despawn MUST NOT error
- duplicate Insert/Remove MUST not create oscillation
- duplicate Update MUST not break determinism

---

### [entity-replication-10] — Identity reuse safety (LocalEntity wrap/reuse)
Local entity identifiers (HostEntity/RemoteEntity) may wrap/reuse over time.

Naia MUST ensure:
- Late or reordered replication actions from an old lifetime cannot corrupt a new lifetime, even if LocalEntity IDs are reused.
- Some lifetime-disambiguating information MUST gate applicability of replication actions to the correct lifetime.

Non-normative note:
- A common strategy is to gate by tick boundaries (spawn/despawn tick), but the contract is the invariant: **no cross-lifetime corruption**.

---

### [entity-replication-11] — GlobalEntity rollover is a terminal error
GlobalEntity is treated as effectively unique.

If the server’s monotonic GlobalEntity counter would roll over:
- Naia MUST NOT silently wrap/reuse GlobalEntity values.
- Naia MUST enter a **terminal error mode** (fail-fast / abort / panic), because continued operation would violate identity stability.

This is intentionally strict: rollover is astronomically unlikely and correctness beats availability here.

---

### [entity-replication-12] — Conflict resolution: server wins for replicated state
If a conflict occurs between client-local state and server-replicated state for any replicated component:

- The server’s replicated state MUST overwrite the client’s local state (convergence requirement).

Additional design constraint (to avoid conflicts by construction):
- While an entity is client-owned and not delegated, the server SHOULD NOT originate replicated component mutations for that entity except those derived from accepted owner writes and server-driven lifecycle transitions (scope/publish/delegation/despawn). If it does, the “server wins” rule still applies.

- Delegated authority refinement:
    - For delegated entities, the server’s outbound replicated state remains the canonical convergence source for all clients.
    - While a client holds authority (Granted/Releasing), the server MUST treat the authority holder’s accepted writes as the source for that canonical replicated state (plus lifecycle transitions).
    - Therefore, the server MUST NOT originate independent conflicting replicated component mutations for `E` while a client holds authority.
    - If the server revokes/resets authority, the canonical source may transition back to server-originated state after the reset boundary (see `11_entity_authority.md`).

---

## Test obligations (TODO placeholders)

For each contract above, Naia MUST eventually have at least one E2E test proving it.

- entity-replication-01 — TODO: stable GlobalEntity across lifetime
- entity-replication-02 — TODO: lifetime boundaries; fresh spawn after ≥1 tick out-of-scope
- entity-replication-03 — TODO: Spawn contains full baseline state
- entity-replication-04 — TODO: no observable update/insert/remove before Spawn
- entity-replication-05 — TODO: late/out-of-lifetime actions ignored
- entity-replication-06 — TODO: update-before-insert buffered then applied
- entity-replication-07 — TODO: local-only overwritten by server replication
- entity-replication-08 — TODO: collapse to final per tick; no intermediate states
- entity-replication-09 — TODO: duplicates idempotent
- entity-replication-10 — TODO: LocalEntity reuse cannot corrupt new lifetime
- entity-replication-11 — TODO: GlobalEntity rollover fail-fast (unit-level)
- entity-replication-12 — TODO: server-wins convergence for replicated state

---

## Cross-references

- `6_entity_scopes.md` — defines scope enter/leave semantics and the ≥1 tick lifetime rule
- `9_entity_publication.md` — defines publish/unpublish interactions with scope
- `8_entity_ownership.md` — defines local-only mutation rules and ownership write constraints
- `10_entity_delegation.md` / `11_entity_authority.md` — define delegation and authority semantics
- `13_client_events_api.md` — defines client-observable event ordering/meaning
- `5_time_ticks_commands.md` — defines tick semantics (including wrap considerations)
