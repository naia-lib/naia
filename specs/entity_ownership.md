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

Ownership is per-entity and exclusive. It is queryable via `entity(...).owner()` on both server and client. The server’s view is more detailed; the client’s view is intentionally coarse.

---

## Core Contracts

### Ownership is per-entity, exclusive, and not per-component
- **entity-ownership-01**: Ownership MUST be defined per-Entity and MUST NOT be defined per-Component.
- **entity-ownership-01**: An Entity MUST have exactly one owner at any moment (exclusive ownership).

### Client-owned entities (server view)
- **entity-ownership-02**: For a **client-owned Entity E**, the server MUST accept **writes** for E only from the owning client and MUST NOT apply writes from any other client.
- **entity-ownership-02**: The server MAY ignore unauthorized writes silently and/or record a metric/log, but MUST NOT apply them.

### Server-owned entities (server view)
- **entity-ownership-03**: For a **server-owned Entity E** (not client-owned), the server MUST NOT accept **writes** for E from any client.
- **entity-ownership-03**: The server MAY ignore unauthorized writes silently and/or record a metric/log, but MUST NOT apply them.

### Ownership does not emit authority events for client-owned entities
- **entity-ownership-04**: Ownership alone MUST NOT emit Authority events for client-owned entities. Authority events are part of Delegation/Authority, not Ownership.

---

## Client-side Safety Rules (Panic Contracts)

### Clients must never write unowned entities
- **entity-ownership-05**: A client MUST NOT write (replicate over the wire) any update for an Entity it does not own.
- **entity-ownership-05**: If Naia would enqueue/serialize/send a replication write for an unowned entity, Naia MUST panic.

This is a hard invariant: Naia guarantees well-behaved clients never attempt such writes.

### Ownership visibility on the client is intentionally coarse
- **entity-ownership-06**: On the client, `entity(...).owner()` MUST return an `EntityOwner` enum.
- **entity-ownership-06**: For the client, any entity not owned by that client MUST be reported as `EntityOwner::Server` (i.e., the client MUST NOT observe “owned by another client”).
- **entity-ownership-06**: Client-owned entities visible to the owning client MUST be reported as `EntityOwner::Client`.
- **entity-ownership-06**: Local-only entities MUST be reported as `EntityOwner::Local`.

(As of the current public server API, the server’s `EntityOwner` has richer variants such as `Client`, `ClientWaiting`, and `ClientPublic`.) :contentReference[oaicite:0]{index=0}

---

## Mutate vs Write Behavior on Clients (Local Prediction & Local-Only State)

### Non-owners may mutate locally, but must never write
- **entity-ownership-07**: A client MAY mutate entities it does not own (insert/remove/update components, and despawn locally), but such mutations MUST NOT write/replicate to the server.
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

### Client-owned entities are inherently always in-scope for the owner client
- **entity-ownership-12**: While the owning client is connected, a client-owned entity MUST be considered in-scope for that owning client (the owning client must never lose scope for its client-owned entities).

---

## Disconnect Handling

### Owner disconnect despawns all client-owned entities
- **entity-ownership-13**: When a client disconnects, the server MUST despawn all entities owned by that client.
- **entity-ownership-13**: There are no exceptions (delegation/authority do not change this ownership rule).

---

## Out-of-scope / unpublished write attempts

- **entity-ownership-14**: A client MUST NOT write about an entity it does not own.
- **entity-ownership-14**: Naia MUST guarantee that a client does not write about entities that are out-of-scope/unpublished and unowned by it; if such a write would occur, Naia MUST panic.

Exception note: `EntityProperty` may refer to entities as data (identity/reference semantics). This is a read/reference mechanism and MUST NOT be treated as “writing an entity the client does not own.”

---

## Test Obligations (TODO)

(We are not implementing tests yet; these are placeholders.)

- **entity-ownership-02/03**: Unauthorized client writes MUST NOT affect server state.
- **entity-ownership-05**: Client MUST panic if it would write an unowned entity.
- **entity-ownership-08**: Local-only component persists until despawn; server replication overwrites if it begins replicating later.
- **entity-ownership-09**: Client MUST panic on unauthorized removal of a server-replicated component from an unowned entity.
- **entity-ownership-13**: Owner disconnect despawns all client-owned entities.