# Entity Publication

Entity Publication defines the **only valid semantics** for whether a *client-owned* entity may be replicated (spawned/updated) to **non-owning clients**.

Publication is a **gate** layered on top of scoping:
- **Scoping** decides *which* clients are in-scope.
- **Publication** decides whether non-owners are even *eligible* to be in-scope for a client-owned entity.

This spec is intentionally narrow:
- It defines publication as a closed, normative contract.
- It does **not** redefine ownership, scopes, replication, or delegation; it cross-references them.

---

## 1) Scope

### In scope
- Publication states and transitions for **client-owned** entities.
- Required effect of publication on **non-owner scope eligibility**.
- Observable publication state via `replication_config()` on server/client entities.

### Out of scope (defined elsewhere)
- Ownership write acceptance / panics (`08_entity_ownership.spec.md`)
- Scope computation & in-scope/out-of-scope mechanics (`06_entity_scopes.spec.md`)
- Replication ordering / wire semantics (`07_entity_replication.spec.md`)
- Delegation migration & delegated authority (`10_entity_delegation.spec.md`, `11_entity_authority.spec.md`)

---

## 2) Vocabulary

- **Owner(E)**: The owner of entity `E` (see `08_entity_ownership.spec.md`).
- **Owning client A**: A client `A` such that `Owner(E) == A`.
- **Non-owner client C**: A client `C` such that `C != Owner(E)`.
- **InScope(C,E)** / **OutOfScope(C,E)**: defined in `06_entity_scopes.spec.md`.
- **Despawn (client-side)**: `E` is removed from the client’s networked entity pool (and all of its components in that pool are destroyed).
- **Publication state (client-owned only)**:
  - **Published**: the server MAY scope `E` to non-owners (subject to scope policy).
  - **Unpublished**: the server MUST NOT scope `E` to any non-owner.

### Observable: ReplicationConfig
Naia exposes an observable replication configuration via `replication_config() -> Option<ReplicationConfig>` and a setter `configure_replication(ReplicationConfig)` on server & client entity handles.

This spec defines how `ReplicationConfig::{Private,Public,Delegated}` maps onto publication semantics **only for client-owned entities**.

---

## 3) Contract (Rules)

### [entity-publication-01] — Publication gates only client-owned visibility to non-owners

**Obligations:**
- **t1**: Publication gates only client-owned visibility to non-owners works correctly
Publication semantics apply only to **client-owned** entities as a gate for **non-owner** visibility.
This spec does not impose additional constraints on server-owned entities beyond what `06_entity_scopes.spec.md` / `07_entity_replication.spec.md` specify.

### [entity-publication-02] — Unpublished client-owned entities are never in-scope for non-owners

**Obligations:**
- **t1**: Unpublished client-owned entities are never in-scope for non-owners works correctly
If `E` is client-owned and **Unpublished** with owner `A`:
- for all clients `C != A`, `OutOfScope(C,E)` MUST hold.

### [entity-publication-03] — Published client-owned entities may be in-scope for non-owners

**Obligations:**
- **t1**: Published client-owned entities may be in-scope for non-owners works correctly
If `E` is client-owned and **Published** with owner `A`:
- the server MAY place `E` into scope of clients `C != A` per normal scope policy.

### [entity-publication-04] — Only the server or owning client may change publication; server wins conflicts

**Obligations:**
- **t1**: Only the server or owning client may change publication; server wins conflicts works correctly
Only the server OR the owning client MAY cause `E` to transition:
- Unpublished ↔ Published

If the server and owning client produce conflicting publication changes “in the same effective replication window”
(e.g. within one server tick / one resolved change-set), the server’s final resolved publication state MUST win.

Notes:
- There is no requirement that publication transitions are exposed as a public API; they MAY be system-driven.
- This rule defines *authority to cause the transition*, not how the API is shaped.

### [entity-publication-05] — Unpublish forces immediate OutOfScope for all non-owners

**Obligations:**
- **t1**: Unpublish forces immediate OutOfScope for all non-owners works correctly
When client-owned `E` transitions **Published → Unpublished**:
- all non-owner clients MUST become `OutOfScope(C,E)` for `C != Owner(E)` as part of the next resolved scope update.

### [entity-publication-06] — Publish enables later scoping; does not guarantee scoping

**Obligations:**
- **t1**: Publish enables later scoping; does not guarantee scoping works correctly
When client-owned `E` transitions **Unpublished → Published**:
- the server MAY later scope `E` to non-owners per policy;
- publication does not itself guarantee that any particular non-owner becomes in-scope.

### [entity-publication-07] — Owning client is always in-scope for its owned entities

**Obligations:**
- **t1**: Owning client is always in-scope for its owned entities works correctly

For any client-owned entity `E` with owner `A`:
- `InScope(A,E)` MUST always hold while `A` is connected.
- Publication MUST NOT remove `E` from the owning client's scope.
- Setting `replication_config(E)` to `Private` MUST NOT remove `E` from owner's scope.

**This is an absolute invariant (restated from `06_entity_scopes.spec.md`):** Publication/scoping MUST NOT hide an entity from its owner.

(If the entity ceases to exist—e.g. it is despawned—this rule no longer applies.)

**Observable signals:**
- Owning client never receives despawn for owned entity due to publication changes

**Test obligations:**
- `entity-publication-07.t1`: Owning client retains visibility when setting entity to Private

### [entity-publication-08] — Non-owner unpublish/out-of-scope implies despawn and destroys local-only components

**Obligations:**
- **t1**: Non-owner unpublish/out-of-scope implies despawn and destroys local-only components works correctly
If a non-owner client `C != Owner(E)` transitions to `OutOfScope(C,E)` due to publication becoming Unpublished:
- `E` MUST despawn on that client (be removed from the client’s networked entity pool).
- All components attached to `E` in that client’s pool (including any “local-only” components) MUST be destroyed.

This is intentionally aligned with the general “OutOfScope ⇒ despawn” rule in `06_entity_scopes.spec.md`;
publication is just one cause of OutOfScope.

### [entity-publication-09] — Publication MUST be observable via replication_config

**Obligations:**
- **t1**: Publication MUST be observable via replication_config
For a client-owned entity `E` that exists on the server:
- `Published` MUST correspond to `replication_config(E) == Some(Public)`
- `Unpublished` MUST correspond to `replication_config(E) == Some(Private)`

For a non-owner client `C != Owner(E)`:
- If `E` exists in the client’s networked entity pool, then `replication_config(E)` MUST NOT be `Some(Private)`.
  (Because `Some(Private)` would mean Unpublished, which must be OutOfScope for non-owners.)

### [entity-publication-10] — Delegation migration ends “client-owned publication” semantics

**Obligations:**
- **t1**: Delegation migration ends “client-owned publication” semantics works correctly
If a client-owned entity `E` migrates into a **delegated server-owned entity** (see `10_entity_delegation.spec.md`):
- `E` is no longer client-owned, and publication semantics in this spec no longer apply.
- Non-owners are no longer gated by “Published/Unpublished client-owned rules”; the entity is now governed by
  server-owned scoping + delegated rules.

Cross-constraint (restated for coherence; the detailed rule lives in `10_entity_delegation.spec.md`):
- A client-owned entity MUST be Published before it may migrate into delegated server-owned form.

---

## 4) Illegal cases & required behavior

This section exists to prevent “undefined behavior pockets.” These situations MUST NOT occur in correct Naia usage,
but if they do occur due to a bug or misuse, behavior is still defined.

### [entity-publication-11] — If a non-owner observes a client-owned Private entity, it MUST be treated as OutOfScope

**Obligations:**
- **t1**: If a non-owner observes a client-owned Private entity, it MUST be treated as OutOfScope
If a non-owner client `C != Owner(E)` ever reaches a state where:
- `E` exists in the client’s networked entity pool AND `replication_config(E) == Some(Private)`

then the client MUST immediately treat `E` as `OutOfScope(C,E)` and despawn it.

Rationale: this restores the invariant required by entity-publication-02/09 without relying on perfect server behavior.

---

## State Transition Table: Publication (Client-Owned Entities)

| Current State | Trigger | Who Can Trigger | Next State | Effect on Non-Owners |
|---------------|---------|-----------------|------------|----------------------|
| Unpublished | configure_replication(Public) | Owner or Server | Published | MAY enter scope per policy |
| Published | configure_replication(Private) | Owner or Server | Unpublished | MUST exit scope immediately |
| Published | configure_replication(Delegated) | Owner or Server | (Delegated) | Ownership transfers to server |
| (any) | Owner disconnects | (automatic) | (despawned) | Entity despawned globally |

---

## 5) Test obligations (TODO placeholders; not implementing yet)

- **entity-publication-02**: Prove unpublished client-owned entities never appear for non-owners.
- **entity-publication-05/08**: Prove Published→Unpublished forces non-owner despawn, destroying local-only components.
- **entity-publication-06**: Prove Unpublished→Published does not guarantee any non-owner in-scope.
- **entity-publication-07**: Prove owning client always retains in-scope visibility across publication toggles.
- **entity-publication-09**: Prove `replication_config` accurately reflects Published/Public and Unpublished/Private.
- **entity-publication-10**: Prove delegated migration requires Published first and then switches to delegated semantics.
- **entity-publication-11**: Prove the client self-heals by despawning if it ever sees `Private` on a non-owned entity.

---

## 6) Cross-references

- Ownership: `08_entity_ownership.spec.md`
- Scopes: `06_entity_scopes.spec.md`
- Replication ordering/wire behavior: `07_entity_replication.spec.md`
- Delegation & authority: `10_entity_delegation.spec.md`, `11_entity_authority.spec.md`