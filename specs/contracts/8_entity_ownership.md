# Entity Ownership

This spec defines **Entity Ownership**: which actor is permitted to **write** replicated state for an Entity.

Ownership is **not** Delegation, and ownership is **not** Authority. Those are specified elsewhere. Ownership is the coarse, per-entity "who may write replicated updates" rule; Delegation/Authority describe finer-grained permission flows and events.

---

## Definitions

### Mutate vs Write

- **Mutate**: change the local world state by inserting/removing/updating components and/or despawning an entity.
- **Write**: cause a mutation to be **replicated over the wire** (serialized into outbound replication and sent to the remote host).

A mutation may be allowed locally (mutate) while still being forbidden to replicate (write).

### Replicated component vs local-only component

- A **replicated component** is a component type registered for replication in the Protocol.
- A **local-only component** is any component instance that exists only in a local world view and is not currently backed by replicated authority for that entity on that host (even if its type is a replicated type).

Local-only components may exist on entities a host does not own.

### Owner

Ownership is per-entity and exclusive. It is queryable via `entity(...).owner()` on both server and client.

### EntityOwner (ownership-only)

`EntityOwner` is a statement of **who owns the entity**, and it MUST be independent of publication / scope / replication configuration.

- `EntityOwner::Server` — server-owned entity.
- `EntityOwner::Client(UserKey)` — client-owned entity (owned by the specified user).
- `EntityOwner::Local` — local-only entity (never networked; MUST NOT participate in Naia replication, publication, scopes, delegation, or authority).

**Normative:**
- `server.entity(entity).owner()` MUST return only: `Server | Client(UserKey) | Local`.
- `client.entity(entity).owner()` MUST return:
  - `Client(<this client's UserKey>)` for client-owned entities owned by this client.
  - `Server` for all entities not owned by this client (including entities owned by other clients).
  - `Local` only for local-only entities (which MUST NOT interact with Naia networking).

---

## Core Contracts

### [entity-ownership-01] — Ownership is per-entity, exclusive, and not per-component

Ownership MUST be defined per-Entity and MUST NOT be defined per-Component. An Entity MUST have exactly one owner at any moment (exclusive ownership).

**Observable signals:**
- `entity(...).owner()` returns a single `EntityOwner` value

**Test obligations:**
- `entity-ownership-01.t1`: Verify an entity has exactly one owner at creation and cannot have multiple owners

---

### [entity-ownership-02] — Server accepts writes only from owning client (client-owned entities)

For a **client-owned Entity E**, the server MUST accept **writes** for E only from the owning client and MUST NOT apply writes from any other client.

The server MAY ignore unauthorized writes silently and/or record a metric/log, but MUST NOT apply them.

**Observable signals:**
- Server state remains unchanged after unauthorized write attempt
- (Debug only) Warning may be emitted

**Test obligations:**
- `entity-ownership-02.t1`: Unauthorized client write attempts do not affect server state

---

### [entity-ownership-03] — Server rejects writes for non-delegated server-owned entities

For any server-owned entity `E` that is NOT delegated (`replication_config(E) != Some(Delegated)`), the server MUST NOT accept replicated writes from any client for `E`. Such writes MUST be ignored/dropped.

For delegated entities, client writes are governed by `10_entity_delegation.md` / `11_entity_authority.md` (authority holder may write; others must not).

The server MAY ignore unauthorized writes silently and/or record a metric/log, but MUST NOT apply them.

**Observable signals:**
- Server state remains unchanged after unauthorized write attempt
- (Debug only) Warning may be emitted

**Test obligations:**
- `entity-ownership-03.t1`: Client writes to non-delegated server-owned entities are ignored

---

### [entity-ownership-04] — Ownership alone does not emit authority events

Ownership alone MUST NOT emit Authority events for client-owned entities. Authority events are part of Delegation/Authority, not Ownership.

**Observable signals:**
- No authority events emitted for non-delegated client-owned entities

**Test obligations:**
- `entity-ownership-04.t1`: Client-owned entity creation does not trigger authority events

---

## Client-side Write Permission

### [entity-ownership-05] — Client write permission rules

A client MUST NOT write/replicate entity updates unless it is a permitted writer for that entity.

A client is a permitted writer for entity `E` iff:
- `owner(E) == EntityOwner::Client(this_client)`, OR
- `replication_config(E) == Some(Delegated)` AND `authority(E) ∈ {Granted, Releasing}`.

**Error handling:**
- If user code attempts to trigger a replication write for an entity the client is not permitted to write, Naia MUST return `Result::Err` from the initiating API call.
- If Naia's internal replication path would enqueue/serialize/send a replication write from a client that is not a permitted writer (framework invariant violation), Naia MUST panic.

Cross-link:
- Delegated authority write permission is defined in `10_entity_delegation.md` / `11_entity_authority.md`.

**Observable signals:**
- API returns `Err` for unauthorized write attempts
- Internal invariant violations cause panic (framework bug)

**Test obligations:**
- `entity-ownership-05.t1`: User API call to write unowned entity returns `Err`
- `entity-ownership-05.t2`: Internal write path for unowned entity panics (internal invariant test)

---

### [entity-ownership-06] — Ownership visibility on client is coarse

On the client, `entity(...).owner()` MUST return an `EntityOwner` enum:
- For the client, any entity not owned by that client MUST be reported as `EntityOwner::Server` (i.e., the client MUST NOT observe "owned by another client").
- Client-owned entities visible to the owning client MUST be reported as `EntityOwner::Client`.
- Local-only entities MUST be reported as `EntityOwner::Local`.

**Observable signals:**
- `entity(...).owner()` returns coarse-grained ownership

**Test obligations:**
- `entity-ownership-06.t1`: Client sees other clients' entities as `Server`-owned

---

## Mutate vs Write Behavior on Clients (Local Prediction & Local-Only State)

### [entity-ownership-07] — Non-owners may mutate locally but must never write

A client MAY mutate entities it does not own (insert/remove/update components), but such mutations MUST NOT write/replicate to the server.

Any replicated updates received from the server for that entity MUST overwrite the client's local state for the relevant replicated components.

**Observable signals:**
- Local mutations persist until server overwrites
- No outbound replication for non-owned entities

**Test obligations:**
- `entity-ownership-07.t1`: Local mutation on non-owned entity persists until server update
- `entity-ownership-07.t2`: Server update overwrites local mutation

---

### [entity-ownership-08] — Local-only components persist until despawn or server replication

If a client inserts a component (replicated or non-replicated type) onto an entity it does not own, and the server never replicates that component for that entity, the component MUST persist locally until:
- removed locally (allowed), or
- the entity despawns (scope-leave/unpublish/etc), which destroys all local-only components.

If the server later begins replicating that component for that entity, the newly replicated "official" component state MUST overwrite the existing local-only component state. This overwrite MUST be treated as a **component Insert** in client events/observability (not an Update).

**Observable signals:**
- Component Insert event when server replication overwrites local-only component

**Test obligations:**
- `entity-ownership-08.t1`: Local-only component persists until despawn
- `entity-ownership-08.t2`: Server replication overwrites local-only component with Insert event

---

### [entity-ownership-09] — Removing replicated components from unowned entities

A client MAY remove a component from an unowned entity only if that component instance is local-only on that client.

**Error handling:**
- If a client attempts to remove a **replicated component instance** that was originally supplied by the server (i.e., it exists in the entity due to replication), Naia MUST return `Result::Err` from the remove API call.
- If Naia's internal path would remove a server-replicated component from an unowned entity (framework invariant violation), Naia MUST panic.

Rationale: removing a server-replicated component locally creates a misleading "phantom delete" that cannot be written, and would be immediately contradicted by subsequent replication.

**Observable signals:**
- API returns `Err` for unauthorized remove attempts

**Test obligations:**
- `entity-ownership-09.t1`: Removing server-replicated component from unowned entity returns `Err`

---

## Ownership Transitions

### [entity-ownership-10] — Server-owned entities never migrate to client-owned

An entity that is server-owned MUST NOT transition to client-owned at any time.

**Observable signals:**
- No ownership change event from server to client ownership

**Test obligations:**
- `entity-ownership-10.t1`: Server-owned entity cannot become client-owned

---

### [entity-ownership-11] — Client-owned entities may migrate to server-owned delegated

A client-owned entity MAY transition to server-owned (delegated) only when delegation is enabled for that entity by:
- the owning client, or
- the server (server authority takes priority).

When delegation is enabled for a client-owned entity:
- ownership MUST transfer from client → server as part of that action.
- once a client-owned entity transfers to server ownership via delegation enabling, it MUST NOT revert back to client ownership.

Note: "delegated" here describes the downstream Authority/permission model; ownership itself is simply "server-owned" after the transfer.

**Observable signals:**
- `owner()` changes from `Client` to `Server` when delegation is enabled

**Test obligations:**
- `entity-ownership-11.t1`: Enabling delegation on client-owned entity transfers ownership to server
- `entity-ownership-11.t2`: Delegated entity cannot revert to client ownership

---

### [entity-ownership-12] — Owning client always in-scope for its entities

A client MUST always see its own client-owned entities as in-scope (they MUST NOT be despawned due to scope changes on that owning client).

For non-owner clients, when an entity leaves scope (unpublish/room divergence/exclude/etc), the entity MUST despawn client-side.

**Observable signals:**
- Owning client never receives despawn for owned entity while connected
- Non-owners receive despawn on scope exit

**Test obligations:**
- `entity-ownership-12.t1`: Owning client retains owned entities across scope changes
- `entity-ownership-12.t2`: Non-owner despawns entity on scope exit

---

## Disconnect Handling

### [entity-ownership-13] — Owner disconnect despawns all client-owned entities

When a client disconnects, the server MUST despawn all entities owned by that client. There are no exceptions (delegation/authority do not change this ownership rule).

**Observable signals:**
- Entity despawn events on server after owner disconnect
- Other clients observe despawn for those entities

**Test obligations:**
- `entity-ownership-13.t1`: Client disconnect despawns all client-owned entities on server
- `entity-ownership-13.t2`: Other clients observe despawn for disconnected client's entities

---

## Out-of-scope / Unpublished Write Attempts

### [entity-ownership-14] — No writes for out-of-scope entities

A client MUST NOT write/replicate updates for any entity that it is not a permitted writer for (see `entity-ownership-05`).

Naia MUST guarantee it never attempts to write/replicate for entities that are out-of-scope on that client; if such a write would occur, Naia MUST panic (framework invariant violation).

Exception note: `EntityProperty` may refer to entities as data (identity/reference semantics). This is a read/reference mechanism and MUST NOT be treated as "writing an entity the client does not own."

**Observable signals:**
- Panic on internal invariant violation (framework bug)

**Test obligations:**
- `entity-ownership-14.t1`: Internal attempt to write out-of-scope entity panics

---

## Test obligations

Each contract above includes inline test obligations. Summary:
- `entity-ownership-01.t1`: Exclusive ownership per entity
- `entity-ownership-02.t1`: Unauthorized writes rejected
- `entity-ownership-03.t1`: Non-delegated server-owned writes rejected
- `entity-ownership-04.t1`: No authority events for non-delegated owned entities
- `entity-ownership-05.t1`: User API returns Err for unauthorized write
- `entity-ownership-05.t2`: Internal invariant panics for unauthorized write
- `entity-ownership-06.t1`: Coarse ownership visibility on client
- `entity-ownership-07.t1`: Local mutation persists; `entity-ownership-07.t2`: Server overwrites
- `entity-ownership-08.t1`: Local-only persists; `entity-ownership-08.t2`: Server overwrites with Insert
- `entity-ownership-09.t1`: Removing server-replicated component returns Err
- `entity-ownership-10.t1`: Server-owned cannot become client-owned
- `entity-ownership-11.t1/t2`: Delegation migration transfers ownership
- `entity-ownership-12.t1/t2`: Owner in-scope; non-owner despawns
- `entity-ownership-13.t1/t2`: Disconnect despawns owned entities
- `entity-ownership-14.t1`: Out-of-scope write panics (internal invariant)

---

## Cross-references

- Scopes: `6_entity_scopes.md`
- Publication: `9_entity_publication.md`
- Delegation: `10_entity_delegation.md`
- Authority: `11_entity_authority.md`
- Events: `12_server_events_api.md`, `13_client_events_api.md`
- Error taxonomy: `0_common.md`
