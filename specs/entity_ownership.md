# Spec: Entity Ownership
Defines the only valid semantics for non-delegated entity ownership and write acceptance.

**Status:** Active  
**Version:** v1  
**Related specs:** `entity_publication.md`, `entity_delegation.md`, `entity_authority.md`, `entity_scopes.md`, `entity_replication.md`  
**Applies to:** server + client

**Contract ID format:** `entity-ownership-<nn>` (stable; never reused; never renumbered)

---

## 1) Scope & Vocabulary

**In scope**
- Canonical ownership for **non-delegated** entities.
- Write acceptance rules for server-owned undelegated and client-owned entities.

**Out of scope**
- Delegation enable/disable and migration (see `entity_delegation.md`)
- Authority statuses/events (see `entity_authority.md`)
- Scope membership rules (see `entity_scopes.md`)

**Vocabulary**
- **Entity `E`**: a replicated object with a stable identity.
- **Owner(E)**: single writer-of-record for a **non-delegated** entity: `Server` or `Client(A)`.
- **Client-owned**: entity with `Owner(E)=Client(A)`.
- **Server-owned undelegated**: entity with `Owner(E)=Server` and not delegated.

---

## 2) Contract (Rules)

### entity-ownership-01 — Ownership is exclusive
An entity MUST be exactly one of:
- server-owned, or
- client-owned by exactly one client.

### entity-ownership-02 — Undelegated entities have no authority concept
For all **non-delegated** entities (server-owned undelegated and all client-owned states):
- “authority status” is undefined and MUST NOT be queried, stored, inferred, or surfaced.
(Authority exists only for delegated entities; see `entity_authority.md`.)

### entity-ownership-03 — Server-owned undelegated write acceptance
For a server-owned undelegated entity `E`:
- the server is authoritative;
- all client writes MUST be rejected/ignored.

### entity-ownership-04 — Client-owned write acceptance
For a client-owned entity `E` with `Owner(E)=Client(A)`:
- the server MUST accept writes from `A`;
- the server MUST reject/ignore writes from any `C != A`.

(Visibility/scoping depends on publication; see `entity_publication.md`.)

---

## 3) Contract IDs (Obligations)

### entity-ownership-01 — Ownership is exclusive
**Guarantee:** exactly one owner domain applies per entity.  
**Covered by tests:** downstream via domain tests (ownership implied across all suites).

### entity-ownership-02 — No authority for non-delegated
**Guarantee:** no authority status/events for non-delegated entities.  
**Covered by tests:** `test/tests/entity_client_owned.rs::client_owned_entities_emit_no_authority_events` and undelegated tests in authority/delegation suites.

### entity-ownership-03 — Server-owned undelegated rejects client writes
**Guarantee:** client mutation attempts do not affect authoritative state.  
**Covered by tests:** (delegation toggle / undelegated suite; exact mapping lives in E2E plan/tests)

### entity-ownership-04 — Client-owned accepts only owner writes
**Guarantee:** owner writes accepted; non-owner writes ignored.  
**Covered by tests:** `test/tests/entity_client_owned.rs::client_owned_published_rejects_non_owner_mutations` and `...accepts_owner_mutations_and_propagates`

---

## 4) Interfaces & Observability

- Clients MAY attempt writes; acceptance follows rules above.
- Rejected/ignored writes MUST NOT panic.
- No “authority” API applies here (see `entity_authority.md`).

---

## 5) Invariants & Non-Goals

**Always true**
- Ownership is entity-level (not per-component).

**Non-goals**
- Does not define scoping, delegation, authority, or message ordering.

---

## 6) Changelog
- v1: extracted from prior omnibus contract.
