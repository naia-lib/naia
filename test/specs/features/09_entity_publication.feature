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
# NORMATIVE PUBLICATION RULES:
#   [entity-publication-01] Publication gates only client-owned visibility
#     - Applies only to client-owned entities as non-owner gate
#
#   [entity-publication-02] Unpublished entities are never in-scope for non-owners
#     - If E is Unpublished, OutOfScope(C,E) MUST hold for all C != Owner
#
#   [entity-publication-03] Published entities may be in-scope for non-owners
#     - Server MAY place E into non-owner scope per normal policy
#
#   [entity-publication-04] Only server or owning client may change publication
#     - Server wins conflicts within same tick
#
#   [entity-publication-05] Unpublish forces immediate OutOfScope for non-owners
#     - Published → Unpublished: all non-owners become OutOfScope
#
#   [entity-publication-06] Publish enables later scoping, does not guarantee it
#     - Unpublished → Published: server MAY later scope to non-owners
#
#   [entity-publication-07] Owning client is always in-scope for owned entities
#     - InScope(owner, entity) MUST always hold while connected
#     - Private setting MUST NOT remove from owner's scope
#
#   [entity-publication-08] Non-owner out-of-scope implies despawn + destroy local
#     - Despawn destroys all components including local-only
#
#   [entity-publication-09] Publication observable via replication_config
#     - Published → replication_config == Some(Public)
#     - Unpublished → replication_config == Some(Private)
#
#   [entity-publication-10] Delegation migration ends client-owned publication
#     - E becomes server-owned, publication semantics no longer apply
#     - Must be Published before migration
#
# ILLEGAL CASES:
#   [entity-publication-11] Non-owner seeing Private must self-heal
#     - Client MUST immediately despawn if it observes Private on non-owned
#
# ============================================================================

Feature: Entity Publication

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Unpublished entities are never in-scope for non-owners
  # --------------------------------------------------------------------------
  # NORMATIVE: If E is Unpublished, OutOfScope(C,E) MUST hold for all non-owners.
  # --------------------------------------------------------------------------
  Rule: Unpublished entities are never in-scope for non-owners

    Scenario: Unpublished client-owned entity is invisible to non-owners
      Given a client-owned entity set to Private
      And a non-owner client in the same room
      Then the entity is not visible to the non-owner

  # --------------------------------------------------------------------------
  # Rule: Published entities may be scoped to non-owners
  # --------------------------------------------------------------------------
  # NORMATIVE: Published entities MAY be placed in non-owner scope.
  # --------------------------------------------------------------------------
  Rule: Published entities may be scoped to non-owners

    Scenario: Published client-owned entity can be visible to non-owners
      Given a client-owned entity set to Public
      And a non-owner client in the same room
      Then the entity is visible to the non-owner

  # --------------------------------------------------------------------------
  # Rule: Unpublish forces immediate OutOfScope for non-owners
  # --------------------------------------------------------------------------
  # NORMATIVE: Published → Unpublished forces all non-owners OutOfScope.
  # --------------------------------------------------------------------------
  Rule: Unpublish forces immediate OutOfScope for non-owners

    Scenario: Publishing then unpublishing causes non-owner despawn
      Given a published client-owned entity visible to non-owner
      When the entity is set to Private
      Then the non-owner observes despawn
      And all components including local-only are destroyed

  # --------------------------------------------------------------------------
  # Rule: Owning client is always in-scope for owned entities
  # --------------------------------------------------------------------------
  # NORMATIVE: InScope(owner, entity) MUST always hold. Private does not
  # remove from owner's scope.
  # --------------------------------------------------------------------------
  Rule: Owning client is always in-scope for owned entities

    Scenario: Owning client retains visibility when setting to Private
      Given a client-owned entity
      When the entity is set to Private
      Then the owning client still sees the entity

  # --------------------------------------------------------------------------
  # Rule: Publication is observable via replication_config
  # --------------------------------------------------------------------------
  # NORMATIVE: Published → Public, Unpublished → Private in replication_config.
  # --------------------------------------------------------------------------
  Rule: Publication is observable via replication_config

    Scenario: replication_config reflects publication state
      Given a client-owned entity
      When the entity is set to Public
      Then replication_config returns Public
      When the entity is set to Private
      Then replication_config returns Private

  # --------------------------------------------------------------------------
  # Rule: Delegation migration requires Published first
  # --------------------------------------------------------------------------
  # NORMATIVE: Client-owned entity MUST be Published before delegation.
  # --------------------------------------------------------------------------
  Rule: Delegation migration requires Published first

    Scenario: Delegation requires Published state
      Given a Private client-owned entity
      When attempting to enable delegation
      Then the operation fails or requires publishing first

    Scenario: Delegating published entity succeeds
      Given a Published client-owned entity
      When delegation is enabled
      Then the entity becomes server-owned delegated

  # --------------------------------------------------------------------------
  # Rule: Non-owner seeing Private must despawn
  # --------------------------------------------------------------------------
  # NORMATIVE: If non-owner ever observes Private, it MUST despawn.
  # --------------------------------------------------------------------------
  Rule: Non-owner seeing Private must despawn

    Scenario: Non-owner observing Private self-heals by despawning
      Given a scenario where non-owner might observe Private
      Then the client despawns the entity immediately

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified. The entity publication spec is comprehensive.
# ============================================================================
