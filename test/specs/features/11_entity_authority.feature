# ============================================================================
# Entity Authority — Canonical Contract
# ============================================================================
# Source: contracts/11_entity_authority.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines how a client can acquire and release the right
#   to write replicated updates for a server-owned delegated entity, and what
#   each side can observe about that right. Defines the EntityAuthStatus state
#   machine and can_write/can_read derived capabilities.
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
#   Define the authority state machine (EntityAuthStatus), client request/
#   release semantics, server-controlled authority, and required behavior.
#
# GLOSSARY:
#   - EntityAuthStatus (client-visible):
#     * Available: No client holds authority; may request
#     * Requested: Client requested, awaiting server decision (optimistic)
#     * Granted: Client holds authority
#     * Releasing: Client initiated release, awaiting confirmation
#     * Denied: Authority held by another client or server
#   - can_write: True iff endpoint owns entity OR is active authority holder
#   - can_read: True iff endpoint is NOT the active authority holder
#
# ----------------------------------------------------------------------------
# CORE AUTHORITY RULES
# ----------------------------------------------------------------------------
#
# Authority defined only for delegated entities:
#   - If replication_config(E) != Delegated, authority(E) MUST be None
#   - Request/release on non-delegated MUST return error
#
# Single-writer rule (client-side):
#   - Client MAY write only when Granted or Releasing
#   - Writing in other states MUST panic
#
# Meaning of Denied:
#   - Authority held by another client OR server
#   - Transitions to Available when holder releases or server resets
#
# ----------------------------------------------------------------------------
# CLIENT API SEMANTICS
# ----------------------------------------------------------------------------
#
# request_authority() is optimistic:
#   - Available → Requested immediately (no round-trip wait)
#
# Request completion transitions:
#   - Requested → Granted (server grants)
#   - Requested → Denied (another holds)
#   - Requested → Available (server resets)
#
# release_authority() transitions:
#   - Granted → Releasing → Available
#   - Requested → Available (cancels request)
#
# Client-side error returns:
#   - Not delegated → ErrNotDelegated
#   - Out-of-scope → ErrNotInScope
#   - Entity doesn't exist → ErrNoSuchEntity
#
# ----------------------------------------------------------------------------
# SERVER SEMANTICS
# ----------------------------------------------------------------------------
#
# First-request wins arbitration
#
# Server may hold authority and block clients:
#   - All clients in Denied while server holds
#
# Server override/reset:
#   - Granted/Releasing/Denied → Available (revoked)
#   - Requested → Available (cleared)
#
# ----------------------------------------------------------------------------
# SCOPE/LIFETIME/DISCONNECT
# ----------------------------------------------------------------------------
#
# Out-of-scope ends authority for that client:
#   - Entity lifetime ends, status cleared, buffered actions discarded
#
# Authority holder losing scope forces global release:
#   - Other in-scope clients: Denied → Available
#
# Delegation disable clears authority:
#   - Authority becomes None, pending cleared, grants revoked
#
# ----------------------------------------------------------------------------
# ROBUSTNESS
# ----------------------------------------------------------------------------
#
# Out-of-scope requests ignored server-side:
#   - Silent in production, MAY warn in Debug mode
#
# Duplicate/late signals are idempotent:
#   - No duplicate effects, converge to final state
#
# ----------------------------------------------------------------------------
# OBSERVABILITY
# ----------------------------------------------------------------------------
#
# Authority observable via status and events
#
# STATE TRANSITION TABLE:
#   Available + request_authority() → Requested (can_write=false, can_read=true)
#   Requested + Server grants → Granted (can_write=true, can_read=false)
#   Requested + Server denies → Denied (can_write=false, can_read=true)
#   Requested + Server resets → Available (can_write=false, can_read=true)
#   Granted + release_authority() → Releasing (can_write=true, can_read=false)
#   Granted + Server resets → Available (can_write=false, can_read=true)
#   Granted + Lose scope → (cleared)
#   Releasing + Server confirms → Available (can_write=false, can_read=true)
#   Denied + Holder releases → Available (can_write=false, can_read=true)
#   Denied + Server resets → Available (can_write=false, can_read=true)
#
# ============================================================================


Feature: Entity Authority

  # All executable scenarios deferred until step bindings implemented.

