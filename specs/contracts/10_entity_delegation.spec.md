# Entity Delegation

Entity Delegation defines how a **server-owned delegated entity** grants temporary **Authority** to clients so that
exactly one client at a time may **write** replicated updates for that entity.

Delegation is distinct from:
- **Ownership**: who ultimately owns the entity (see `08_entity_ownership.spec.md`).
- **Publication**: whether client-owned entities are visible to non-owners (see `09_entity_publication.spec.md`).
- **Scope**: whether an entity exists on a client at all (see `06_entity_scopes.spec.md`).
- **Replication**: spawn/update ordering and lifetime rules (see `07_entity_replication.spec.md`).

This spec defines:
- the meaning of the `Delegated` replication configuration
- authority arbitration (request/grant/deny/release)
- required client/server behavior and observability

---

## 1) Glossary

- **Delegated entity**: a server-owned entity configured as `ReplicationConfig::Delegated`.
- **Authority holder**: the single actor currently allowed to **write** replicated updates for the delegated entity.
  The authority holder is either:
    - the server, or
    - exactly one client.
- **Authority status (client view)**: an `EntityAuthStatus` value that expresses a client’s current permission state
  with respect to writing:
    - `Available`: no one is currently holding authority; client may request.
    - `Requested`: client requested authority; not yet granted.
    - `Granted`: client currently holds authority and may write.
    - `Releasing`: authority is in the process of being released; writing may still be permitted until release finalizes.
    - `Denied`: authority is currently held by another client or by the server, so this client cannot request/grab it.

Non-normative note:
- The shared delegation state machine includes `can_mutate()` and `can_write()` distinctions; clients can mutate earlier
  than they can write. This spec defines the observable meaning of those states.

---

## 2) Core Model

### [entity-delegation-01] — Delegation applies only to server-owned delegated entities

**Obligations:**
- **t1**: Delegation applies only to server-owned delegated entities.
Authority delegation semantics apply only when:
- the entity is server-owned, and
- `replication_config(E) == Some(Delegated)`.

If an entity is not delegated, this spec’s authority arbitration does not apply.

### [entity-delegation-02] — Single-writer invariant

**Obligations:**
- **t1**: Single-writer invariant.
For any delegated entity `E`, at any time:
- at most one client MAY be the authority holder for `E`.
- the server MAY reset/revoke authority at any time (see `11_entity_authority.spec.md`).
- the server MAY hold authority (server-as-holder) which forces all clients to observe `Denied`.
- while a client holds authority (Granted/Releasing), the server MUST NOT originate independent replicated component writes for `E`; the server’s replicated state MUST be derived from the current authority holder’s accepted writes plus server-driven lifecycle transitions.

Client-visible implication:
- exactly one client can have `EntityAuthStatus::Granted` at a time for a given delegated entity.

### [entity-delegation-03] — Authority is scoped: only in-scope clients participate

**Obligations:**
- **t1**: Authority is scoped: only in-scope clients participate.
Only clients for which `InScope(U,E)` holds MAY request authority for `E`.

If a client is out-of-scope for `E`, it MUST NOT request authority for `E` and MUST NOT be granted authority for `E`.

---

## 3) Entering Delegation (Migration)

### [entity-delegation-04] — Client-owned → server-owned delegated migration requires Published

**Obligations:**
- **t1**: Client-owned → server-owned delegated migration requires Published.
A client-owned entity MUST be Published/`Public` before it may migrate into a server-owned delegated entity.

(Ownership/publication constraints are defined in `08_entity_ownership.spec.md` and `09_entity_publication.spec.md`;
this rule is restated here as a delegation precondition.)

### [entity-delegation-05] — Migration grants authority to previous owner

**Obligations:**
- **t1**: Migration grants authority to previous owner.
When a client-owned, Published entity `E` migrates into a server-owned delegated entity:
- ownership transfers to the server (per `08_entity_ownership.spec.md`).
- the previous owner client MUST immediately become the authority holder.
- on that previous owner client, `EntityAuthStatus(E)` MUST be `Granted`.

When a client-owned, published entity migrates to server-owned delegated, the previous owner client MUST immediately start with `EntityAuthStatus::Granted` for that entity.

Rationale:
- delegation migration should not create a behavior cliff for the former owner.

---

## 4) Authority Arbitration (Request/Grant/Deny/Release)

### [entity-delegation-06] — First request wins

**Obligations:**
- **t1**: First request wins.
If `E` is delegated and currently has no client authority holder (i.e., authority is `Available`):
- the first in-scope client to request authority MUST be granted authority.
- while a client holds authority, no other client may be granted authority until it is released or reset.

### Authority requests are NOT queued

Normative:
- If a delegated entity’s authority is currently held by some holder (client or server),
  then **the server MUST NOT transfer authority to a different client** just because that client requests authority.
- Requests made while authority is held MUST resolve as `Denied` for the requester (i.e., “someone else holds it”).
- When the current holder releases authority (or the server revokes/releases it), the entity becomes `Available`.
  - Other clients do NOT automatically receive authority.
  - A client MUST call `request_authority()` again while `Available` to obtain authority.

### [entity-delegation-07] — Meaning of Denied

**Obligations:**
- **t1**: Meaning of Denied.
For a client `C` and delegated entity `E`:
- `EntityAuthStatus(C,E) == Denied` MUST mean: authority is currently held by another client OR by the server.
- A client in `Denied` status MUST remain denied until authority is released or reset by the holder or the server,
  at which point the status MUST transition back to `Available`.

This is not a “request rejection” outcome; it is a “currently unavailable” outcome.

### [entity-delegation-08] — Requested means pending; no writes allowed

**Obligations:**
- **t1**: Requested means pending; no writes allowed.
When a client requests authority and is in `Requested`:
- the client MAY mutate locally (prediction/local prep) but MUST NOT write replicated updates.
- if Naia would attempt to write while in `Requested`, it MUST panic.

### [entity-delegation-09] — Granted means writes allowed; single writer enforced

**Obligations:**
- **t1**: Granted means writes allowed; single writer enforced.
When a client is in `Granted` for delegated entity `E`:
  - that client MAY write replicated updates for `E`.
  - all other clients MUST be in `Denied` for `E` (or `Available` only if not tracking the entity’s status explicitly).
  - While a client is `Granted`/`Releasing` for `E`, the authority holder is the sole origin of replicated component updates for `E`.
  - The server MUST NOT attempt to ‘override’ by sending conflicting component writes while the client holds authority.
  - If the server needs to override, it MUST first reset/revoke authority (`entity-authority-10`), optionally become the holder (`entity-authority-09`), and then replicate its authoritative state.

### [entity-delegation-10] — Releasing means writes may still occur until release finalizes

**Obligations:**
- **t1**: Releasing means writes may still occur until release finalizes.
When a client enters `Releasing`:
- the client MAY continue to write replicated updates until the release is finalized,
  after which it MUST become `Available`.
- other clients MUST remain `Denied` until the release finalizes and authority becomes `Available`.

### [entity-delegation-11] — Release transitions authority back to Available

**Obligations:**
- **t1**: Release transitions authority back to Available.
If the authority holder releases authority (or the server releases/resets it):
- the authority state MUST become `Available`.
- all clients that were `Denied` due to another holder MUST transition to `Available`.

---

## 5) Client Safety (Panic Contracts)

### [entity-delegation-12] — Client must never write without permission

**Obligations:**
- **t1**: Client must never write without permission
If Naia would enqueue/serialize/send a replication write for a delegated entity `E` from a client that is not permitted
to write (`EntityAuthStatus != Granted/Releasing`):
- Naia MUST panic.

This is a hard invariant: Naia framework controls writing and must enforce this strictly.

---

## 6) Scope/Disconnect Interactions

### [entity-delegation-13] — Losing scope ends client authority

**Obligations:**
- **t1**: Losing scope ends client authority.
If a client that holds authority for `E` becomes out-of-scope for `E`:
- authority MUST be released/reset by the server.
- other in-scope clients MUST transition to `Available` (subject to first-request wins on new requests).

Cross-link:
- Scope transitions and despawn semantics are defined in `06_entity_scopes.spec.md`.

### [entity-delegation-14] — Disconnect releases authority

**Obligations:**
- **t1**: Disconnect releases authority.
If the authority-holding client disconnects:
- the server MUST release/reset authority for `E`.
- other in-scope clients MUST transition to `Available`.

If the disconnected client also owned client-owned entities, those are despawned globally per `08_entity_ownership.spec.md`.
This rule concerns only delegated server-owned entities.

---

## 7) Observability (Events & Queryability)

### [entity-delegation-17] — Delegation observability

**Obligations:**
- **t1**: Delegation observability.

Delegation MUST be observable through:
- `replication_config(E) == Some(Delegated)` (server + client observable)
- authority status and events (defined in `11_entity_authority.spec.md` and the events API specs)

This spec defines the required semantics; the concrete event types and delivery guarantees are specified in:
- `12_server_events_api.spec.md`
- `13_client_events_api.spec.md`
- `11_entity_authority.spec.md`

---

## 8) Illegal / Misuse Cases

### [entity-delegation-15] — Requesting authority while out-of-scope is ignored (warn in Debug mode)

**Obligations:**
- **t1**: Requesting authority while out-of-scope is ignored (warn in Debug mode).
If a client requests authority for `E` while out-of-scope:
- server MUST ignore the request silently in production.
- server MAY emit a warning when Debug mode are enabled.

### [entity-delegation-16] — Conflicting reconfiguration is resolved by server final state

**Obligations:**
- **t1**: Conflicting reconfiguration is resolved by server final state.
If configuration changes (e.g., toggling Delegated on/off) would produce conflicting intermediate states within a tick:
- the server MUST collapse to the final resolved state per tick, consistent with `07_entity_replication.spec.md` and
  `06_entity_scopes.spec.md`.
- clients MUST observe only the final state transitions (no intermediate oscillations).

---

## 9) Test obligations (TODO placeholders; not implementing yet)

- entity-delegation-04: migration requires Published
- entity-delegation-05: migration grants authority to previous owner client
- entity-delegation-06: first request wins; others denied
- entity-delegation-07/11: denied persists until release; release returns others to Available
- entity-delegation-08/12: write while not allowed panics
- entity-delegation-10: releasing allows writes until finalized
- entity-delegation-13/14: losing scope/disconnect releases authority and unblocks others
- entity-delegation-15: out-of-scope requests ignored (warn only in Debug mode)
- entity-delegation-16: same-tick collapse yields final-only observability

---

## 10) Cross-references

- Ownership: `08_entity_ownership.spec.md`
- Publication: `09_entity_publication.spec.md`
- Scopes: `06_entity_scopes.spec.md`
- Replication: `07_entity_replication.spec.md`
- Authority & events: `11_entity_authority.spec.md`, `12_server_events_api.spec.md`, `13_client_events_api.spec.md`