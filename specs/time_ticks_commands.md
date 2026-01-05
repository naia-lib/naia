# Time, Ticks & Commands Contract

This document defines the **only** valid semantics for time progression, tick processing, command submission/history, and (if enabled) prediction/correction behavior.  
Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHALL**, **SHOULD**.

---

## Glossary

- **Tick**: a discrete simulation step with an integer index.
- **Server Tick**: the server’s authoritative tick index.
- **Client Tick**: the client’s local notion of time progression (may be ahead/behind server tick, depending on configuration).
- **Tick Rate**: configured frequency at which ticks advance.
- **Command**: a client-authored input/event intended to affect simulation state, typically associated with a tick index.
- **Command Stream**: an ordered sequence of commands for a given client (or entity), indexed by tick.
- **Command History Window**: the bounded retention window for past commands.
- **Prediction**: client-side simulation ahead of authoritative confirmation.
- **Correction**: authoritative state received that differs from predicted state for an earlier tick.
- **Replay**: re-applying retained commands after a correction to recompute the present predicted state.

---

## Pipeline Position (Normative)

This spec constrains:
1) Time/tick advancement and the mapping between real time and tick index  
2) Command acceptance, indexing, ordering, and retention  
3) (If enabled) prediction/correction/replay semantics and their observable guarantees

Related specs:
- `entity_replication.md` (authoritative state delivery and ordering constraints)
- `transport.md` (fault model: loss/dup/reorder/jitter)
- `messaging.md` (request/response and channel ordering where used by commands)
- `server_events_api.md` / `client_events_api.md` (event exposure per tick)

---

## Contracts

### time-01 — Tick indices are monotonic
**Rule:** Server Tick and Client Tick MUST be monotonic non-decreasing within a single process lifetime.  
**MUST NOT:** decrease, roll back, or “rewind” the tick index as observed by public APIs.

**Tests (obligations):**
- `time-01.t1` — Server tick monotonically increases under normal progression.
- `time-01.t2` — Client tick monotonically increases under normal progression.

---

### time-02 — No phantom ticks while time is paused
**Rule:** If time progression is paused (no elapsed time / deterministic clock not advanced), tick indices MUST NOT advance.

**Tests (obligations):**
- `time-02.t1` — Pause then resume produces no extra ticks during pause and continues from last tick.

---

### time-03 — Deterministic clock yields deterministic tick schedule
**Rule:** Under a deterministic time source (test clock), identical inputs (time deltas, commands, transport behavior) MUST produce identical tick advancement and externally observable outcomes.

**Tests (obligations):**
- `time-03.t1` — Running the same scripted scenario twice yields identical tick counts and observable events.

---

### time-04 — Tick-step atomicity (authoritative sequencing)
**Rule:** For each Server Tick, the server MUST apply its simulation step atomically such that:
- all state updates for that tick are internally consistent, and
- externally observable outputs for that tick are derived from that atomic result.

This spec does not mandate exact internal ordering between subsystems, but it DOES require the result exposed to clients/events to be consistent per tick.

**Tests (obligations):**
- `time-04.t1` — Multi-update in a single tick is observed as a coherent state at clients (no impossible mixed partials).

---

### time-05 — Tick wraparound is safe (if supported)
**Rule:** If tick indices are implemented with wraparound (bounded integer type), wraparound MUST preserve ordering semantics as defined by the engine’s tick comparison rules.  
**MUST NOT:** panic, deadlock, or misorder tick-grouped processing across wrap.

**Notes:** If the system uses an unbounded tick type, this contract is trivially satisfied.

**Tests (obligations):**
- `time-05.t1` — Tick ordering across wrap remains correct (if wrap exists).

---

## Command Submission & Ordering

### commands-01 — Commands are indexed to a tick
**Rule:** Every accepted command MUST be associated with a specific tick index (explicitly or implicitly assigned by the system) and MUST be ordered by that index within a Command Stream.

**Tests (obligations):**
- `commands-01.t1` — Commands observed/processed are attributable to a tick index and ordered by tick.

---

### commands-02 — Commands for a tick are processed at most once
**Rule:** Within a Command Stream, a command identified as belonging to tick `T` MUST be applied at most once to the authoritative simulation and at most once to any predicted simulation instance.

**MUST NOT:** duplicate-apply due to packet duplication or retries.

**Tests (obligations):**
- `commands-02.t1` — Duplicate deliveries do not cause duplicate application.
- `commands-02.t2` — Retries do not cause reapplication.

---

### commands-03 — Late commands outside the acceptance window are handled predictably
**Rule:** Commands that arrive “too late” (outside the server’s accepted tick window) MUST be handled in a defined way:
- MUST be rejected/ignored OR
- MUST be clamped/remapped per a documented policy.

The chosen behavior MUST be consistent and MUST NOT corrupt state.

**Tests (obligations):**
- `commands-03.t1` — Late command beyond window is rejected/ignored (or remapped) consistently.

---

### commands-04 — Command History is bounded and drops old commands
**Rule:** Command retention MUST be bounded by a Command History Window. Commands older than the window MUST be discarded, and replay MUST NOT depend on discarded commands.

**Tests (obligations):**
- `commands-04.t1` — Old commands beyond window are discarded.
- `commands-04.t2` — Corrections older than the window do not attempt to replay discarded commands.

---

### commands-05 — Disconnect cancels in-flight command stream safely
**Rule:** On disconnect, any in-flight or buffered commands associated with that connection MUST be safely cleaned up.
**MUST NOT:** leak unbounded memory/state or apply commands after the connection is disconnected.

**Tests (obligations):**
- `commands-05.t1` — Disconnect cleans command buffers and no further commands apply post-disconnect.

---

## Prediction / Correction / Replay (Only If Enabled)

> These contracts apply only when prediction/correction is enabled in the build/config.  
> If prediction is not supported, tests for this section may be absent or explicitly not applicable.

### predict-01 — Correction is anchored to an authoritative tick
**Rule:** Any correction received MUST identify the authoritative tick (or authoritative snapshot tick) it corresponds to.

**Tests (obligations):**
- `predict-01.t1` — Correction references a concrete authoritative tick.

---

### predict-02 — Replaying commands after correction preserves order
**Rule:** When a correction for tick `Tc` is applied and the client retains commands for ticks `Tc+1..Tn`, the client MUST replay those commands in increasing tick order to recompute its predicted state.

**Tests (obligations):**
- `predict-02.t1` — After correction, replay order is strictly by tick.

---

### predict-03 — No regression to older state after newer state applied
**Rule:** Once the client has advanced its predicted state to incorporate authoritative/corrected state for tick `T` (or beyond), it MUST NOT later regress to an older state for ticks `< T` due to delayed packets or reordering.

**Tests (obligations):**
- `predict-03.t1` — Delayed older corrections/updates do not regress the client after it has applied newer authoritative state.

---

### predict-04 — Corrections older than retained history are handled safely
**Rule:** If a correction arrives for a tick older than the Command History Window, the client MUST handle it safely:
- MAY snap to authoritative state without replay OR
- MAY ignore with a defined error/metric,
but MUST NOT panic or corrupt state.

**Tests (obligations):**
- `predict-04.t1` — Out-of-window correction is handled without panic and with defined behavior.

---

### predict-05 — Replay is deterministic given same inputs
**Rule:** Given the same authoritative correction and the same retained command sequence, replay MUST produce the same predicted state.

**Tests (obligations):**
- `predict-05.t1` — Replay determinism under fixed inputs.

---

## Observability

### obs-01 — Public API tick reporting is consistent
**Rule:** Any public-facing tick exposure (events, debug counters, metrics, or callbacks) MUST reflect the monotonic tick model and MUST be self-consistent within a tick.

**Tests (obligations):**
- `obs-01.t1` — Events/metrics report a coherent tick number per step and do not jump backward.

---

## Illegal / Edge Cases (Defined Behavior)

### illegal-01 — Out-of-range tick indices do not crash the system
**Rule:** If malformed or out-of-range tick indices are received (e.g., via corrupted transport), the system MUST reject/ignore safely without panicking.

**Tests (obligations):**
- `illegal-01.t1` — Malformed tick indices are handled safely.

---

### illegal-02 — Command stream misuse is safe
**Rule:** If a client submits commands in a way that violates API expectations (e.g., duplicate tick index with conflicting payload where not supported), the system MUST return a defined error or reject/ignore safely.

**Tests (obligations):**
- `illegal-02.t1` — Conflicting duplicate commands are rejected/ignored safely with defined outcome.

---

## Forbidden Behaviors

- Tick indices decreasing as observed by any public API.
- Duplicate application of the same logical command due to duplication/retry.
- Unbounded growth of command buffers/history under normal operation.
- Panics caused by malformed tick indices, out-of-window corrections, or late commands.
- Regressing client-visible state to an older tick after newer authoritative state has been applied.
