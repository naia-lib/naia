# ============================================================================
# Entity Publication — Canonical Contract
# ============================================================================
# Source: contracts/09_entity_publication.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines whether a client-owned entity may be replicated
#   to non-owning clients. Publication is a gate layered on top of scoping:
#   Scoping decides which clients are in-scope; Publication decides whether
#   non-owners are eligible to be in-scope for client-owned entities.
#
# Terminology note:
#   This file is normative; scenarios are executable assertions; comments
#   labeled NORMATIVE are part of the contract.
# ============================================================================

# ============================================================================
# NORMATIVE CONTRACT MIRROR
# ============================================================================
#
# PURPOSE:
#   Define publication states and transitions for client-owned entities.
#
# GLOSSARY:
#   - Owner(E): The owner of entity E
#   - Owning client A: Client such that Owner(E) == A
#   - Non-owner client C: Client such that C != Owner(E)
#   - Published: Server MAY scope E to non-owners
#   - Unpublished: Server MUST NOT scope E to any non-owner
#   - ReplicationConfig::Private: Unpublished
#   - ReplicationConfig::Public: Published
#   - ReplicationConfig::Delegated: Published + delegation enabled
#
# SCOPE:
#   In scope: Publication states/transitions for client-owned entities
#   Out of scope: Ownership, scopes, replication, delegation (defined elsewhere)
#
# ----------------------------------------------------------------------------
# PUBLICATION GATING RULES
# ----------------------------------------------------------------------------
#
# Publication gates only client-owned visibility:
#   - Applies only to client-owned entities as non-owner gate
#
# Unpublished entities are never in-scope for non-owners:
#   - If E is Unpublished, OutOfScope(C,E) MUST hold for all C != Owner
#
# Published entities may be in-scope for non-owners:
#   - Server MAY place E into non-owner scope per normal policy
#
# ----------------------------------------------------------------------------
# PUBLICATION CONTROL
# ----------------------------------------------------------------------------
#
# Only server or owning client may change publication:
#   - Server wins conflicts within same tick
#
# Unpublish forces immediate OutOfScope for non-owners:
#   - Published → Unpublished: all non-owners become OutOfScope
#
# Publish enables later scoping, does not guarantee it:
#   - Unpublished → Published: server MAY later scope to non-owners
#
# ----------------------------------------------------------------------------
# OWNER VISIBILITY GUARANTEE
# ----------------------------------------------------------------------------
#
# Owning client is always in-scope for owned entities:
#   - InScope(owner, entity) MUST always hold while connected
#   - Private setting MUST NOT remove from owner's scope
#
# Non-owner out-of-scope implies despawn + destroy local:
#   - Despawn destroys all components including local-only
#
# ----------------------------------------------------------------------------
# OBSERVABILITY
# ----------------------------------------------------------------------------
#
# Publication observable via replication_config:
#   - Published → replication_config == Some(Public)
#   - Unpublished → replication_config == Some(Private)
#
# ----------------------------------------------------------------------------
# DELEGATION INTERACTION
# ----------------------------------------------------------------------------
#
# Delegation migration ends client-owned publication:
#   - E becomes server-owned, publication semantics no longer apply
#   - Must be Published before migration
#
# ----------------------------------------------------------------------------
# ILLEGAL CASES
# ----------------------------------------------------------------------------
#
# Non-owner seeing Private must self-heal:
#   - Client MUST immediately despawn if it observes Private on non-owned
#
# ============================================================================


@Feature(entity_publication)
Feature: Entity Publication

  # --------------------------------------------------------------------------
  # Rule: Publication gating
  # --------------------------------------------------------------------------
  # Publication gates non-owner visibility for client-owned entities
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Entity Publication

    @Scenario(01)
    Scenario: Unpublished entity is out-of-scope for non-owners
      Given a server is running
      And client A connects
      And client B connects
      And client A spawns a client-owned entity with Private replication config
      And client A and the entity share a room
      And client B and the entity share a room
      Then the entity is out-of-scope for client B

    @Scenario(02)
    Scenario: Published entity may be in-scope for non-owners
      Given a server is running
      And client A connects
      And client B connects
      And client A spawns a client-owned entity with Public replication config
      And client A and the entity share a room
      And client B and the entity share a room
      Then the entity is in-scope for client B

    @Scenario(03)
    Scenario: Owning client always in-scope regardless of publication state
      Given a server is running
      And client A connects
      And client A spawns a client-owned entity with Private replication config
      And client A and the entity share a room
      Then the entity is in-scope for client A

    # [entity-publication-05] — Unpublish forces immediate OutOfScope for non-owners
    # Published → Unpublished: all non-owners MUST become OutOfScope for the entity.
    @Scenario(04)
    Scenario: entity-publication-05 — Unpublish forces entity out-of-scope for non-owners
      Given a server is running
      And client A connects
      And client B connects
      And client A spawns a client-owned entity with Public replication config
      And client A and the entity share a room
      And client B and the entity share a room
      And the entity is in-scope for client B
      When client A unpublishes the entity
      Then the entity becomes out-of-scope for client B

    # [entity-publication-observability-01] — Published entity reports Public replication_config
    # Publication MUST be observable via replication_config on the owning client.
    @Scenario(05)
    Scenario: entity-publication-observability — Public entity reports Public replication_config
      Given a server is running
      And client A connects
      And client A spawns a client-owned entity with Public replication config
      Then client A observes replication config as Public for the entity

    # [entity-publication-observability-02] — Unpublished entity reports Private replication_config
    # Publication MUST be observable via replication_config on the owning client.
    @Scenario(06)
    Scenario: entity-publication-observability — Private entity reports Private replication_config
      Given a server is running
      And client A connects
      And client A spawns a client-owned entity with Private replication config
      Then client A observes replication config as Private for the entity

