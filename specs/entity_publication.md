# Spec: Entity Publication
Defines the only valid semantics for publication/unpublication and visibility constraints.

**Status:** Active  
**Version:** v1  
**Related specs:** `entity_ownership.md`, `entity_scopes.md`, `entity_replication.md`, `entity_delegation.md`  
**Applies to:** server + client

**Contract ID format:** `entity-publication-<nn>` (stable; never reused; never renumbered)

---

## 1) Scope & Vocabulary

**In scope**
- Publication states for client-owned entities.
- Scoping eligibility constraints derived from publication.

**Out of scope**
- Ownership write acceptance (see `entity_ownership.md`)
- Delegation migration constraints beyond “must be published first” (see `entity_delegation.md`)
- Authority (see `entity_authority.md`)

**Vocabulary**
- **Published (client-owned only)**: server MAY scope `E` to non-owning clients.
- **Unpublished (client-owned only)**: server MUST NOT scope `E` to any non-owning client.
- **InScope(C,E)** / **OutOfScope(C,E)**: defined in `entity_scopes.md`.

---

## 2) Contract (Rules)

### entity-publication-01 — Server-owned entities are scoping-eligible
All server-owned entities MUST be scoping-eligible for any client, subject to server scope policy.
(“Published/unpublished” does not apply to server-owned entities.)

### entity-publication-02 — Unpublished client-owned is owner+server only
If `E` is client-owned unpublished with owner `A`:
- for all clients `C != A`, `OutOfScope(C,E)` MUST hold.

### entity-publication-03 — Published client-owned may be scoped to non-owners
If `E` is client-owned published with owner `A`:
- the server MAY place `E` into scope of clients `C != A` per normal scope policy.

### entity-publication-04 — Publication transitions are server/owner initiated and may be automatic
Only the server OR the owning client MAY cause:
- Unpublished ↔ Published
This transition MAY be automatic/system-driven and is not required to be a public API.

### entity-publication-05 — Unpublish forces immediate out-of-scope for non-owners
When client-owned `E` transitions Published → Unpublished:
- all non-owner clients MUST become `OutOfScope(C,E)` for `C != Owner(E)`.

### entity-publication-06 — Publish enables later scoping; does not guarantee scoping
When client-owned `E` transitions Unpublished → Published:
- the server MAY later scope `E` to non-owners per policy;
- publication does not itself guarantee that any particular non-owner becomes in-scope.

### entity-publication-07 — Client-owned entities have no authority statuses/events
For all client-owned entities (published or unpublished):
- authority status is undefined;
- `AuthGranted/AuthDenied/AuthLost` MUST NOT be emitted or surfaced.
(Authority exists only for delegated server-owned entities; see `entity_authority.md`.)

---

## 3) Contract IDs (Obligations)

### entity-publication-02 — Unpublished visibility
**Covered by tests:** `test/tests/entity_client_owned.rs::client_owned_unpublished_is_visible_only_to_owner`

### entity-publication-05 — Unpublish forces despawn for non-owners
**Covered by tests:** `test/tests/entity_client_owned.rs::publish_toggle_published_to_unpublished_forcibly_despawns_for_non_owners`

### entity-publication-07 — No authority for client-owned
**Covered by tests:** `test/tests/entity_client_owned.rs::client_owned_entities_emit_no_authority_events`

(Other obligations map 1:1 to the remaining `entity_client_owned.rs` tests.)

---

## 4) Interfaces & Observability

- Owner and server may trigger publish/unpublish (possibly automatically).
- Non-owners MUST NOT observe unpublished entities.

---

## 5) Invariants & Non-Goals

**Always true**
- Publication never changes ownership by itself.

**Non-goals**
- Does not define delegation or authority.

---

## 6) Changelog
- v1: extracted from prior omnibus contract + replaces older client-owned publication spec.
