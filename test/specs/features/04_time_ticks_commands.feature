# ============================================================================
# Time, Ticks & Commands — Canonical Contract
# ============================================================================
# Source: contracts/04_time_ticks_commands.spec.md
# Last converted: 2026-01-17
#
# Summary:
#   This specification defines Naia's public contract for time sources,
#   tick semantics (server tick, client tick, wrap-around ordering), tick
#   synchronization, client tick-lead targeting, and command acceptance.
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
#   Define the canonical time, tick, and command semantics for Naia.
#
# GLOSSARY:
#   - Time Provider: Naia's time abstraction for monotonic "now" and durations
#   - Instant: Cross-platform monotonic instant type (NOT wall clock)
#   - Duration: Monotonic elapsed time between instants
#   - TickRate: Configured base duration per tick (milliseconds)
#   - Server Tick: Authoritative tick counter maintained by server
#   - Client Tick: Client's tick counter, may adjust pacing for lead targeting
#   - Tick: u16 tick index that wraps around
#   - Command: Client-authored input tagged to a tick
#
# ----------------------------------------------------------------------------
# ERROR HANDLING
# ----------------------------------------------------------------------------
#
#   - User-initiated misuse returns Result::Err
#   - Remote/untrusted anomalies MUST NOT panic
#   - Framework invariant violations MUST panic
#
# ----------------------------------------------------------------------------
# CANONICAL TIME SOURCE
# ----------------------------------------------------------------------------
#
# All durations use Naia's monotonic time provider:
#   - All duration-based behavior (tick advancement, TTL, lead targeting)
#     MUST be derived from Naia's Time Provider, not wall-clock time
#
# Determinism under deterministic time provider:
#   - If Time Provider is deterministic, tick advancement MUST be deterministic
#
# ----------------------------------------------------------------------------
# TICK SEMANTICS
# ----------------------------------------------------------------------------
#
# TickRate is fixed and shared:
#   - TickRate MUST be shared between client and server
#   - TickRate MUST NOT change during connection lifetime
#
# Server Tick advances from elapsed time:
#   - Server MUST NOT "invent" ticks without elapsed time
#   - Server MAY advance multiple ticks if enough time elapsed
#   - Server MUST NOT skip ticks (no silent drop of progression)
#
# Client Tick is monotonic and wrap-safe:
#   - Client tick MUST be monotonic non-decreasing (wrap-safe)
#   - MUST NOT move backwards
#
# Wrap-safe tick ordering rule:
#   - Tick is u16 and wraps
#   - Let diff = (a - b) mod 2^16:
#     * a newer than b iff diff in 1..32767
#     * a equal to b iff diff == 0
#     * a older than b iff diff in 32769..65535
#     * diff == 32768 is ambiguous (treat as "not newer")
#
# ----------------------------------------------------------------------------
# TICK SYNCHRONIZATION
# ----------------------------------------------------------------------------
#
# ConnectEvent implies tick sync complete:
#   - Client MUST NOT emit ConnectEvent until tick sync complete
#   - Client knows server's current tick at connection time
#
# ----------------------------------------------------------------------------
# CLIENT TICK LEAD TARGETING (Overwatch-style)
# ----------------------------------------------------------------------------
#
# Client tick targets a lead ahead of server tick:
#   - target_lead = RTT + (jitter_std_dev * 3) + TickRate
#
# Client pacing may adjust to maintain lead:
#   - Client MAY speed up or slow down pacing
#   - Client MUST remain monotonic
#   - Client MUST converge toward target lead
#
# ----------------------------------------------------------------------------
# COMMAND RULES
# ----------------------------------------------------------------------------
#
# Every command is tagged to a tick
#
# Server applies commands at most once:
#   - Duplicates MUST NOT cause double-application
#
# "Arrives in time" acceptance rule:
#   - Command for tick T is on-time iff received before server begins tick T
#   - On-time: apply during tick T
#   - Late: ignore (no panic, no error to client)
#
# Command sequence is required:
#   - Every command includes sequence number (per-connection, per-tick)
#   - Sequence starts at 0, increments by 1
#   - Encoded as varint
#
# Server applies commands in sequence order:
#   - Apply in ascending sequence order regardless of arrival order
#   - Buffer out-of-order until earlier sequences arrive
#
# Command cap per tick:
#   - MAX_COMMANDS_PER_TICK_PER_CONNECTION = 64 (invariant)
#   - Enqueueing 65th command returns Err
#   - Received sequence >= 64 is dropped (no panic)
#
# Duplicate command handling:
#   - Same (tick, sequence) → first received wins, later dropped
#
# Client lead targeting avoids late commands
#
# Disconnect cleans in-flight command state
#
# ============================================================================

Feature: Time Ticks and Commands

  Background:
    Given a Naia test environment is initialized

  # --------------------------------------------------------------------------
  # Rule: Tick is monotonic and wrap-safe
  # --------------------------------------------------------------------------
  # NORMATIVE: Tick is u16, wraps, and ordering MUST use wrap-safe comparison.
  # --------------------------------------------------------------------------
  Rule: Tick is monotonic and wrap-safe

    Scenario: Wrap-safe ordering holds across wrap boundary
      Given a tick value near the wrap boundary
      When comparing ticks across the wrap
      Then the wrap-safe ordering is correct

    Scenario: Client tick never moves backwards
      Given a connected client
      When observing client tick over time
      Then the client tick never decreases in wrap-safe order

  # --------------------------------------------------------------------------
  # Rule: ConnectEvent implies tick sync complete
  # --------------------------------------------------------------------------
  # NORMATIVE: Client MUST NOT emit ConnectEvent until tick sync is complete.
  # --------------------------------------------------------------------------
  Rule: ConnectEvent implies tick sync complete

    Scenario: Tick sync completes before ConnectEvent
      Given a server with known tick
      When a client connects
      Then the client emits ConnectEvent only after tick sync

  # --------------------------------------------------------------------------
  # Rule: Server tick advances from elapsed time
  # --------------------------------------------------------------------------
  # NORMATIVE: Server MUST advance tick based on elapsed time and TickRate.
  # --------------------------------------------------------------------------
  Rule: Server tick advances from elapsed time

    Scenario: Server tick advances as time elapses
      Given a server with a deterministic time provider
      When time advances by one TickRate duration
      Then the server tick advances by one

    Scenario: Server tick advances by multiple if time catches up
      Given a server with a deterministic time provider
      When time advances by three TickRate durations at once
      Then the server tick advances by three

  # --------------------------------------------------------------------------
  # Rule: Client lead targets ahead of server tick
  # --------------------------------------------------------------------------
  # NORMATIVE: Client tick targets a lead = RTT + jitter*3 + TickRate.
  # --------------------------------------------------------------------------
  Rule: Client lead targets ahead of server tick

    Scenario: Client lead converges toward target
      Given a connected client and server
      And stable network conditions
      When sufficient time passes
      Then the client tick lead converges to approximately the target lead

  # --------------------------------------------------------------------------
  # Rule: Every command is tagged to a tick
  # --------------------------------------------------------------------------
  # NORMATIVE: Every command sent by client MUST be tagged with a tick value.
  # --------------------------------------------------------------------------
  Rule: Every command is tagged to a tick

    Scenario: Commands carry tick tags
      Given a connected client and server
      When the client sends a command
      Then the command is tagged with a tick value

  # --------------------------------------------------------------------------
  # Rule: Server applies commands at most once
  # --------------------------------------------------------------------------
  # NORMATIVE: Duplicate command deliveries MUST NOT cause double-application.
  # --------------------------------------------------------------------------
  Rule: Server applies commands at most once

    Scenario: Duplicate command deliveries do not double-apply
      Given a connected client and server
      And a transport conditioner that duplicates packets
      When the client sends a command
      Then the server applies the command exactly once

  # --------------------------------------------------------------------------
  # Rule: On-time commands are processed, late commands are ignored
  # --------------------------------------------------------------------------
  # NORMATIVE: Command for tick T is on-time iff received before server
  # begins processing tick T. Late commands are ignored.
  # --------------------------------------------------------------------------
  Rule: On-time commands are processed, late commands are ignored

    Scenario: On-time command is processed
      Given a connected client and server
      When the client sends a command for tick T before tick T is processed
      Then the command is applied during tick T

    Scenario: Late command is ignored
      Given a connected client and server
      When the client sends a command for tick T after tick T has been processed
      Then the command is ignored

  # --------------------------------------------------------------------------
  # Rule: Commands are applied in sequence order
  # --------------------------------------------------------------------------
  # NORMATIVE: Server MUST apply commands in ascending sequence order
  # regardless of arrival order.
  # --------------------------------------------------------------------------
  Rule: Commands are applied in sequence order

    Scenario: Reordered packets still apply commands in sequence order
      Given a connected client and server
      And a transport conditioner that reorders packets
      When the client sends commands with sequences 0 1 2
      Then the server applies them in order 0 1 2

  # --------------------------------------------------------------------------
  # Rule: Command cap per tick is enforced
  # --------------------------------------------------------------------------
  # NORMATIVE: MAX_COMMANDS_PER_TICK_PER_CONNECTION = 64. Exceeding returns Err.
  # --------------------------------------------------------------------------
  Rule: Command cap per tick is enforced

    Scenario: Enqueueing 65th command returns Err
      Given a connected client
      When the client enqueues 64 commands for one tick
      And the client attempts to enqueue a 65th command
      Then the enqueue returns an Err result

    Scenario: Received sequence 64 or higher is dropped
      Given a connected client and server
      When the server receives a command with sequence 64
      Then the command is dropped without panic

  # --------------------------------------------------------------------------
  # Rule: Disconnect cleans in-flight command state
  # --------------------------------------------------------------------------
  # NORMATIVE: On disconnect, buffered/in-flight commands are discarded.
  # --------------------------------------------------------------------------
  Rule: Disconnect cleans in-flight command state

    Scenario: Disconnect prevents further command application
      Given a connected client and server
      And the client has sent commands
      When the client disconnects
      Then no further commands from that session are applied

# ============================================================================
# DEFERRED TESTS
# ============================================================================
# Items that cannot be tested with current harness capabilities.
# ============================================================================
#
# Rule: Client tick lead targeting
#   Assertions:
#     - Client lead converges to RTT + jitter*3 + TickRate
#     - Lead convergence within LEAD_CONVERGENCE_TICKS
#   Harness needs: Network conditioner with precise RTT control + metrics API
#
# Rule: Wrap-safe tick ordering at boundary
#   Assertions:
#     - Ordering correct when tick wraps from 65535 to 0
#     - Commands processed correctly across wrap boundary
#   Harness needs: Long-running test or tick injection near wrap boundary
#
# ============================================================================

# ============================================================================
# AMBIGUITIES + PROPOSED CLARIFICATIONS
# ============================================================================
# None identified.
