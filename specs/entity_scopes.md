# Spec: Entity Scope Membership
Defines the only valid semantics for in-scope/out-of-scope presence and its coupling points.

**Status:** Active  
**Version:** v1  
**Related specs:** `entity_publication.md`, `entity_authority.md`, `entity_replication.md`  
**Applies to:** server + client

**Contract ID format:** `entity-scopes-<nn>` (stable; never reused; never renumbered)

---

## 1) Scope & Vocabulary

**In scope**
- Meaning of InScope/OutOfScope and required client-world presence semantics.
- Authority release trigger when holder leaves scope.

**Out of scope**
- How the server decides scope policy (game logic / interest management).
- Publication constraints (see `entity_publication.md`).

**Vocabulary**
- **InScope(C,E)**: client `C` has `E` present in its local world.
- **OutOfScope(C,E)**: `E` is absent (despawned) in client `C`’s local world.
- **Delegated entity**: defined in `entity_delegation.md`.
- **Authority holder**: defined in `entity_authority.md`.

---

## 2) Contract (Rules)

### entity-scopes-01 — Visibility is scope
If `OutOfScope(C,E)`, entity `E` MUST NOT exist in client `C`’s world.
If `InScope(C,E)`, entity `E` MUST exist in client `C`’s world.

### entity-scopes-02 — Client actions require in-scope
A client MUST be able to act on an entity (including authority requests/releases) only if `InScope(C,E)`.

### entity-scopes-03 — Holder leaving scope releases authority
If `E` is delegated and the authority holder is `Client(A)`, and `E` becomes `OutOfScope(A,E)`:
- authority MUST be released (holder becomes None) per `entity_authority.md`.

---

## 3) Contract IDs (Obligations)
(These map primarily into authority/scope coupling tests.)

---

## 4) Interfaces & Observability
- Scope changes are observed as spawn/despawn in client worlds.
- Clients MUST NOT be able to reference OutOfScope entities via normal gameplay APIs.

---

## 5) Invariants & Non-Goals

**Always true**
- Publication may constrain which clients are allowed to be in-scope (see `entity_publication.md`).

**Non-goals**
- Does not define exact timing policy for scope changes (only required effects).

---

## 6) Changelog
- v1: extracted from prior omnibus contract.