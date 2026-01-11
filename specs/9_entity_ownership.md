# Entity Ownership

This spec defines **Entity Ownership**: which actor is permitted to **write** replicated state for an Entity.

Ownership is **not** Delegation, and ownership is **not** Authority. Those are specified elsewhere. Ownership is the coarse, per-entity “who may write replicated updates” rule; Delegation/Authority describe finer-grained permission flows and events.

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
  - `Client(<this client’s UserKey>)` for client-owned entities owned by this client.
  - `Server` for all entities not owned by this client (including entities owned by other clients).
  - `Local` only for local-only entities (which MUST NOT interact with Naia networking).

---

## Core Contracts

### Ownership is per-entity, exclusive, and not per-component
- **entity-ownership-01**: Ownership MUST be defined per-Entity and MUST NOT be defined per-Component.
- **entity-ownership-01**: An Entity MUST have exactly one owner at any moment (exclusive ownership).

### Client-owned entities (server view)
- **entity-ownership-02**: For a **client-owned Entity E**, the server MUST accept **writes** for E only from the owning client and MUST NOT apply writes from any other client.
- **entity-ownership-02**: The server MAY ignore unauthorized writes silently and/or record a metric/log, but MUST NOT apply them.

### Server-owned entities (server view)
- **entity-ownership-03**: For any server-owned entity `E` that is NOT delegated (`replication_config(E) != Some(Delegated)`), the server MUST NOT accept replicated writes from any client for `E`. Such writes MUST be ignored/dropped.
- **entity-ownership-03**: For delegated entities, client writes are governed by `entity_delegation.md` / `entity_authority.md` (authority holder may write; others must not).
- **entity-ownership-03**: The server MAY ignore unauthorized writes silently and/or record a metric/log, but MUST NOT apply them.

### Ownership does not emit authority events for client-owned entities
- **entity-ownership-04**: Ownership alone MUST NOT emit Authority events for client-owned entities. Authority events are part of Delegation/Authority, not Ownership.

---

## Client-side Safety Rules (Panic Contracts)

### Client must never write without permission
- **entity-ownership-05**: A client MUST NOT write/replicate entity updates unless it is a permitted writer for that entity.
- **entity-ownership-05**: A client is a permitted writer for entity `E` iff:
    - `owner(E) == EntityOwner::Client(this_client)`, OR
    - `replication_config(E) == Some(Delegated)` AND `authority(E) ∈ {Granted, Releasing}`.

- **entity-ownership-05**: If Naia would enqueue/serialize/send a replication write from a client that is not a permitted writer: Naia MUST panic.

Cross-link:
- Delegated authority write permission is defined in `entity_delegation.md` / `entity_authority.md`.

### Mutate vs Write (ownership gate)

Definitions:
- **Mutate**: local ECS changes (insert/remove/update components, or despawn) that may be purely local.
- **Write**: producing *replication writes* that are sent over the wire (component field updates, insert/remove replication messages, despawn replication messages).

Normative:
- A client MUST be able to **mutate** unowned entities locally (subject to the rules below).
- A client MUST NEVER **write** replication updates for an entity it does not own.
  - If Naia attempts to write for an unowned entity, it MUST panic (this is a framework-internal invariant violation).

Local-only components on unowned entities:
- If a client inserts any component (replicated or non-replicated) onto an unowned entity, and the server never supplies that component for that entity, the component MUST persist locally until:
  - the client removes it (allowed), or
  - the entity despawns (scope-leave/unpublish/etc), which destroys all local-only components.

Unauthorized removal:
- If a client attempts to remove a **replicated component instance** that was originally supplied by the server (i.e., it exists in the entity due to replication), that removal MUST panic.
- If a client removes a component that exists only locally (never supplied by the server for that entity lifetime), that removal MUST be allowed.

Overwrite by later replication:
- If a local-only component exists and later the server begins replicating that component for the entity, the incoming replicated component MUST overwrite the local-only instance.
- This overwrite MUST be treated as a **component Insert** in client events/observability (not an Update).

### Ownership visibility on the client is intentionally coarse
- **entity-ownership-06**: On the client, `entity(...).owner()` MUST return an `EntityOwner` enum.
- **entity-ownership-06**: For the client, any entity not owned by that client MUST be reported as `EntityOwner::Server` (i.e., the client MUST NOT observe “owned by another client”).
- **entity-ownership-06**: Client-owned entities visible to the owning client MUST be reported as `EntityOwner::Client`.
- **entity-ownership-06**: Local-only entities MUST be reported as `EntityOwner::Local`.

 

---

## Mutate vs Write Behavior on Clients (Local Prediction & Local-Only State)

### Non-owners may mutate locally, but must never write
- **entity-ownership-07**: A client MAY mutate entities it does not own (insert/remove/update components), but such mutations MUST NOT write/replicate to the server.
- **entity-ownership-07**: Any replicated updates received from the server for that entity MUST overwrite the client’s local state for the relevant replicated components.

### Local-only components persist until despawn (even if the type is replicated)
- **entity-ownership-08**: If a client inserts a component (replicated or non-replicated type) onto an entity it does not own, and the server never replicates that component for that entity, the component MUST persist locally until removed locally or the entity is despawned/unpublished.
- **entity-ownership-08**: If the server later begins replicating that component for that entity, the newly replicated “official” component state MUST overwrite the existing local-only component state.

### Removing components from unowned entities: allowed only for local-only components
- **entity-ownership-09**: A client MAY remove a component from an unowned entity only if that component instance is local-only on that client.
- **entity-ownership-09**: If a client attempts to remove a component from an unowned entity where that component instance is currently backed by server replication (i.e., it was inserted/maintained by server replication for that entity), Naia MUST panic.

Rationale: removing a server-replicated component locally creates a misleading “phantom delete” that cannot be written, and would be immediately contradicted by subsequent replication.

---

## Ownership Transitions

### Server-owned entities never migrate to client-owned
- **entity-ownership-10**: An entity that is server-owned MUST NOT transition to client-owned at any time.

### Client-owned entities may migrate to server-owned delegated via enabling delegation
- **entity-ownership-11**: A client-owned entity MAY transition to server-owned (delegated) only when delegation is enabled for that entity by:
  - the owning client, or
  - the server (server authority takes priority).
- **entity-ownership-11**: When delegation is enabled for a client-owned entity, ownership MUST transfer from client → server as part of that action.
- **entity-ownership-11**: Once a client-owned entity transfers to server ownership via delegation enabling, it MUST NOT revert back to client ownership.

Note: “delegated” here describes the downstream Authority/permission model; ownership itself is simply “server-owned” after the transfer.

### Ownership and scope

- A client MUST always see its own client-owned entities as in-scope (they MUST NOT be despawned due to scope changes on that owning client).
- For non-owner clients, when an entity leaves scope (unpublish/room divergence/exclude/etc), the entity MUST despawn client-side.

---

## Disconnect Handling

### Owner disconnect despawns all client-owned entities
- **entity-ownership-13**: When a client disconnects, the server MUST despawn all entities owned by that client.
- **entity-ownership-13**: There are no exceptions (delegation/authority do not change this ownership rule).

---

## Out-of-scope / unpublished write attempts

- **entity-ownership-14**: A client MUST NOT write/replicate updates for any entity that it is not a permitted writer for (see `entity-ownership-05`).
- **entity-ownership-14**: Naia MUST guarantee it never attempts to write/replicate for entities that are out-of-scope on that client; if such a write would occur, Naia MUST panic.

Exception note: `EntityProperty` may refer to entities as data (identity/reference semantics). This is a read/reference mechanism and MUST NOT be treated as “writing an entity the client does not own.”

---

## Test Obligations (TODO)

(We are not implementing tests yet; these are placeholders.)

- **entity-ownership-02/03**: Unauthorized client writes MUST NOT affect server state.
- **entity-ownership-05**: Client MUST panic if it would write an unowned entity.
- **entity-ownership-08**: Local-only component persists until despawn; server replication overwrites if it begins replicating later.
- **entity-ownership-09**: Client MUST panic on unauthorized removal of a server-replicated component from an unowned entity.
- **entity-ownership-13**: Owner disconnect despawns all client-owned entities.