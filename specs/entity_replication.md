# Spec: Entity Replication (Delivery, Ordering, Identity)
Defines the only valid semantics for identity continuity and required delivery/ordering guarantees relevant to entity behavior.

**Status:** Active  
**Version:** v1  
**Related specs:** `entity_scopes.md`, `entity_authority.md`, `entity_publication.md`  
**Applies to:** server + client

**Contract ID format:** `entity-replication-<nn>` (stable; never reused; never renumbered)

---

## 1) Scope & Vocabulary

**In scope**
- Required guarantees about identity continuity and steady-state consistency.
- Required ordering/atomicity at the semantic level for state changes relevant to entities.

**Out of scope**
- Transport-specific mechanics and performance targets (bandwidth, latency).
- Adapter/engine integration details.

**Vocabulary**
- **GlobalEntity**: server canonical entity identifier.
- **LocalEntity**: client-local identifier mapping to a GlobalEntity.
- **Logical identity**: same GlobalEntity represents the same conceptual entity across server/clients.

---

## 2) Contract (Rules)

### entity-replication-01 — Stable logical identity
A GlobalEntity MUST represent the same logical entity for all observers while it exists.

### entity-replication-02 — Mapping consistency in steady state
In steady state, all in-scope clients MUST converge on:
- identical GlobalEntity identity for a given logical entity;
- component state consistent with authoritative replication rules for that entity type.

### entity-replication-03 — Semantic ordering: server tick law for entity state changes
The server MUST compute entity state changes atomically per tick, in this order:
1) Apply scope membership changes.
2) Apply delegation enable/disable and client-owned migration.
3) Apply authority actions (server and client requests/releases).
4) Emit resulting authority statuses/events to in-scope clients per `entity_authority.md`.

### entity-replication-04 — No out-of-order application of authority statuses
Receivers MUST NOT observe out-of-order authority status application.

### entity-replication-05 — Duplicate deliveries must not be required for correctness
Duplicate deliveries MUST NOT be required for correctness; same-to-same status transitions MUST NOT be relied upon.
(This supports the `entity_authority.md` rule that events are transition-driven.)

---

## 3) Contract IDs (Obligations)
(These map primarily to `entities_lifetime_identity.rs` and cross-cutting ordering expectations in authority/delegation tests.)

---

## 4) Interfaces & Observability
- Identity mapping is observable as stable GlobalEntity identity across clients.
- Ordering is observable indirectly via transition-correct behavior (no forbidden intermediate states).

---

## 5) Invariants & Non-Goals

**Always true**
- A client cannot safely depend on internal packet structure; only on the semantic outcomes above.

**Non-goals**
- Does not specify how to implement monotonic id/ack; only that semantic ordering must hold.

---

## 6) Changelog
- v1: replaces older identity spec by folding identity + relevant ordering guarantees.
