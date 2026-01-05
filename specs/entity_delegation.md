# Spec: Entity Delegation
Defines the only valid semantics for enabling/disabling delegation and client-owned → delegated migration.

**Status:** Active  
**Version:** v1  
**Related specs:** `entity_ownership.md`, `entity_publication.md`, `entity_authority.md`, `entity_replication.md`, `entity_scopes.md`  
**Applies to:** server + client

**Contract ID format:** `entity-delegation-<nn>` (stable; never reused; never renumbered)

---

## 1) Scope & Vocabulary

**In scope**
- Server enabling/disabling delegation for server-owned entities.
- Delegating published client-owned entities (migration).

**Out of scope**
- Authority statuses/events once delegated (see `entity_authority.md`).
- Message ordering model (see `entity_replication.md`).

**Vocabulary**
- **Delegated(E)**: entity is in a server-owned delegated mode.
- **Client-owned Published**: required for migration (see `entity_publication.md`).

---

## 2) Contract (Rules)

### entity-delegation-01 — Only server may enable delegation on server-owned undelegated
Only the server MAY enable delegation on a server-owned undelegated entity `E`, producing a delegated entity with no holder.

### entity-delegation-02 — Enable delegation yields Available for in-scope clients
After enabling delegation, all clients `C` with `InScope(C,E)` MUST observe `Available` (per authority broadcast rules in `entity_authority.md`).

### entity-delegation-03 — Only server may disable delegation
Only the server MAY disable delegation on a delegated entity.

### entity-delegation-04 — Disable delegation clears authority semantics immediately
On disable delegation:
- `E` becomes server-owned undelegated;
- authority ceases to exist and MUST NOT remain visible/stored client-side;
- any prior holder is implicitly cleared.

### entity-delegation-05 — Client-owned must be published before delegation (migration)
A client-owned entity MUST be Published before delegation is permitted.
Attempting to delegate an unpublished client-owned entity MUST fail with `ErrNotPublished`.

### entity-delegation-06 — Delegation of client-owned may be initiated by server or owner
Delegation of a client-owned Published entity MAY be initiated by:
- the server, OR
- the owning client.

### entity-delegation-07 — Migration preserves identity (no despawn+spawn)
On delegation of client-owned Published `E`:
- identity MUST be continuous for clients (same logical `E`).

### entity-delegation-08 — Migration result holder depends on owner scope at moment
If owner `A` is in-scope at migration time:
- `E` becomes delegated with holder `Client(A)`.
If owner `A` is out-of-scope at migration time:
- `E` becomes delegated with holder `None`.

### entity-delegation-09 — After migration, scope is server-controlled
After migration, scope membership is controlled solely by server policy; no preservation beyond server policy is guaranteed.

---

## 3) Contract IDs (Obligations)
(These map to the migration-related tests in your Domain 4 suite.)

---

## 4) Interfaces & Observability
- Delegation enable/disable is a server operation.
- Migration is observable as a mode change, not a respawn.

---

## 5) Invariants & Non-Goals

**Always true**
- Delegation applies only to server-owned entities.
- Client-owned entities MUST be Published before delegation.

**Non-goals**
- Does not define authority events/status semantics (see `entity_authority.md`).

---

## 6) Changelog
- v1: extracted from prior omnibus contract.
