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


@Feature(time_ticks_and_commands)
Feature: Time Ticks and Commands

  # --------------------------------------------------------------------------
  # Rule: Command ordering
  # --------------------------------------------------------------------------
  # Server applies commands in sequence order (send order) for the same tick.
  # Commands with the same tick are applied in ascending sequence number order.
  # --------------------------------------------------------------------------
  @Rule(01)
  Rule: Command ordering

    # Tests that multiple commands queued for the same tick are applied in
    # deterministic order (sequence order = send order).
    @Scenario(01)
    Scenario: Multiple commands for same tick are applied in sequence order
      Given a test scenario
      And a server receiving multiple commands for the same tick
      When the tick is processed
      Then commands are applied in receipt order

    # Tests that command processing does not panic regardless of processing
    # complexity. Per contract: "Remote/untrusted anomalies MUST NOT panic"
    @Scenario(02)
    Scenario: Command ordering processing does not cause panic
      Given a test scenario
      And a server receiving multiple commands for the same tick
      When the tick is processed
      Then commands are applied in receipt order
      And no panic occurs

    # Tests that out-of-order arrivals are buffered and applied in sequence order.
    # Per contract: "Apply in ascending sequence order regardless of arrival order"
    # and "Buffer out-of-order until earlier sequences arrive"
    @Scenario(03)
    Scenario: Out-of-order command arrivals are reordered by sequence number
      Given a test scenario
      And a server receiving commands arriving out of order for the same tick
      When the tick is processed
      Then commands are applied in ascending sequence order

    # [time-ticks-03] — ConnectEvent implies tick sync complete
    # Client MUST NOT emit ConnectEvent until tick sync complete.
    # After connection, client_tick() MUST return Some (tick is known).
    @Scenario(04)
    Scenario: time-ticks-03 — Client tick is known after connection
      Given a server is running
      And a client connects
      Then the client tick is available

    # [time-ticks-04] — Client knows server tick at connect time
    # The client knows the server's current tick at connection time (via tick sync).
    @Scenario(05)
    Scenario: time-ticks-04 — Server tick is known to client after connection
      Given a server is running
      And a client connects
      Then the server tick is known to the client

