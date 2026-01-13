# Time, Ticks & Commands

Last updated: 2026-01-09

This spec defines Naia’s public contract for:
- time sources and duration measurement
- tick semantics (server tick, client tick, wrap-around ordering)
- tick synchronization and client tick-lead targeting
- command tick tagging and server acceptance rules

This spec applies to Naia (`naia_server`, `naia_client`). It is transport-agnostic.

Normative keywords: MUST, MUST NOT, MAY, SHOULD.

---

## Scope ownership

This spec owns:
- the canonical time source used for all duration-based behavior
- base tick rate definition and tick advancement rules
- wrap-safe tick ordering and comparison semantics
- the client tick-lead targeting model and how client tick relates to server tick
- command acceptance semantics

This spec does NOT own:
- transport drop/dup/reorder assumptions (see `02_transport.spec.md`)
- message channel ordering/reliability (see `03_messaging.spec.md`)
- entity replication/lifetime (see entity suite)
- connection admission/auth steps (see `01_connection_lifecycle.spec.md`)

---

## Definitions

- **Time Provider**: Naia’s time abstraction used to read a monotonic “now” and measure durations. Tests MAY substitute a deterministic/fake time provider to simulate time passing.

- **Instant**: Naia’s cross-platform monotonic instant type. It MUST NOT be wall clock time.

- **Duration**: monotonic elapsed time between instants.

- **TickRate**: the configured base duration per tick, expressed in milliseconds, shared between client and server. TickRate is fixed for the lifetime of a connection.

- **Server Tick**: the authoritative tick counter maintained by the server, advancing according to TickRate.

- **Client Tick**: the client’s tick counter. The client tracks the same base TickRate, but MAY adjust its pacing to maintain a target lead ahead of the server (see “Client Tick Lead”).

- **Tick**: a `u16` tick index. Tick values wrap around.

- **Command**: client-authored input tagged to a tick.

---

## Global error-handling policy

### [time-01] — User-initiated misuse returns Result::Err
If a failure is caused by local application misuse/configuration at the Naia API layer, Naia MUST return `Result::Err` from the initiating API.

### [time-02] — Remote/untrusted anomalies MUST NOT panic
If a failure is caused by remote input or network behavior (duplicates, reordering, late arrival), Naia MUST NOT panic.
- Prod: ignore/drop silently
- Debug: ignore/drop with warning

### [time-03] — Framework invariant violations MUST panic
If Naia violates an invariant stated in this spec (e.g., tick goes backwards in public API, wrap-order is applied incorrectly, commands are applied more than once), Naia MUST panic.

---

## Canonical time source

### [time-04] — All durations use Naia’s monotonic time provider
All duration-based behavior in Naia (tick advancement, TTL expiry, lead targeting, timeouts if applicable) MUST be derived from Naia’s monotonic Time Provider (Instant/Duration), not wall-clock time.

### [time-05] — Determinism under deterministic time provider
If the Time Provider is deterministic (e.g. in tests), and the sequence of Time Provider advancements is identical, then tick advancement and time-based decisions MUST be deterministic.

---

## Tick semantics

### [time-06] — TickRate is fixed and shared
TickRate is configured as a duration per tick (milliseconds) and MUST be shared between client and server configs for a connection.
TickRate MUST NOT change during a connection’s lifetime.

### [time-07] — Server Tick advances from elapsed time
The server MUST advance its tick counter based on elapsed duration and TickRate.

- The server MUST NOT “invent” ticks without elapsed time.
- The server MAY advance by multiple ticks in one update step if enough time has elapsed.
- The server MUST NOT skip ticks that would have occurred due to elapsed time (no silent drop of tick progression).

(Best-practice note: if the host loop is delayed, processing multiple ticks to catch up is preferred over permanently slowing simulation.)

### [time-08] — Client Tick is monotonic and wrap-safe
The client tick MUST be monotonic non-decreasing in the wrap-safe sense (see time-09). It MUST NOT move backwards.

### [time-09] — Wrap-safe tick ordering rule
Tick is `u16` and wraps. Naia MUST define “newer than / older than” with a wrap-safe comparison:

Let `diff = (a - b) mod 2^16` (u16 wrapping subtraction interpreted as 0..65535).
- `a` is newer than `b` iff `diff` is in `1..32767`.
- `a` is equal to `b` iff `diff == 0`.
- `a` is older than `b` iff `diff` is in `32769..65535`.

Tie-break rule (half-range ambiguity):
- If `diff == 32768` (exactly half range apart), Naia MUST treat `a` as NOT newer than `b` and NOT older than `b` (ambiguous). Implementations MUST NOT rely on ordering in this exact case and MUST choose a deterministic behavior (recommended: treat as “not newer” for eviction / sequencing checks).

---

## Tick synchronization

### [time-10] — ConnectEvent implies tick sync complete
A successful connection handshake MUST include tick synchronization, and the client MUST NOT emit `ConnectEvent` until tick sync is complete. (See `01_connection_lifecycle.spec.md`.)

Tick sync guarantees:
- The client knows the server’s current tick at connection time (or a tick sufficiently recent to compute lead targeting).
- The client can begin maintaining a lead relative to server tick.

---

## Client tick lead targeting (Overwatch-style)

### [time-11] — Client tick targets a lead ahead of server tick
The client MUST attempt to keep its tick ahead of the server by a target lead duration:

`target_lead = RTT + (jitter_std_dev * 3) + TickRate`

Where:
- RTT and jitter_std_dev are estimated by Naia’s connection measurement.
- TickRate is the configured duration-per-tick.

### [time-12] — Client pacing may adjust to maintain lead
To maintain the target lead:
- The client MAY slightly speed up or slow down its tick pacing relative to the base TickRate.
- The client MUST remain monotonic (time-08).
- The client MUST converge toward maintaining `client_tick_time - server_tick_time ≈ target_lead` over time.

This spec does not mandate the exact controller (PID, clamp, etc.), but it DOES mandate the target and monotonicity constraints.

---

## Commands

### [commands-01] — Every command is tagged to a tick
Every command sent by the client MUST be tagged with a tick value.

### [commands-02] — Server applies commands at most once
The server MUST apply a given logical command at most once to authoritative simulation.
Duplicates (retransmits, duplicates at network layer) MUST NOT cause double-application.

### [commands-03] — "Arrives in time" acceptance rule
A command tagged for tick `T` is considered on-time iff it is received by the server before the server begins processing tick `T`.

- If received on-time, the server MUST apply it when processing tick `T`.
- If received late (server has already begun or completed processing tick `T`), the server MUST ignore it.

Ignored late commands are remote/untrusted input outcomes (per `00_common.spec.md`):
- Prod: ignore silently
- Debug: ignore with warning (non-normative)
- MUST NOT panic

(There is no public "rejected command error" surfaced to the client; the contract is that late commands are ignored.)

**Observable signals:**
- Command handler invoked during tick `T` processing if on-time
- No handler invocation for late commands

**Test obligations:**
- `commands-03.t1`: On-time command is processed
- `commands-03.t2`: Late command is ignored

---

### [commands-03a] — Command sequence is required

Every command message MUST include a `sequence` number that identifies its position within a tick.

**Sequence assignment rules:**
- `sequence` is per-connection, per-tick
- `sequence` MUST start at `0` for the first command of each tick
- `sequence` MUST increment by exactly `+1` for each subsequent command in the same tick (no gaps)
- The `(tick, sequence)` pair uniquely identifies a command within a connection

**Wire encoding:**
- `sequence` MUST be encoded as an **unsigned variable-length integer (varint)**.

**Observable signals:**
- `sequence` is observable on received commands

**Test obligations:**
- `commands-03a.t1`: Every command includes a valid `sequence` value

---

### [commands-03b] — Server applies commands in sequence order

**Server ordering rule:**
For a given tick, the server MUST apply commands in ascending `sequence` order (i.e., **send order**), regardless of arrival order on the wire.

**Buffering behavior:**
- If command with `sequence=2` arrives before `sequence=1`, the server MUST buffer `sequence=2` until `sequence=1` arrives
- Once all earlier sequences are received (or tick processing deadline is reached), apply in order

**Observable signals:**
- Command handlers invoked in `sequence` order within each tick
- E2E tests can force packet reordering and still observe deterministic application order

**Test obligations:**
- `commands-03b.t1`: Reordered packets still apply commands in sequence order
- `commands-03b.t2`: Commands are applied in send order regardless of arrival order

---

### [commands-03c] — Command cap per tick

**Invariant constant:**
`MAX_COMMANDS_PER_TICK_PER_CONNECTION = 64`

A sender MUST NOT send more than 64 commands for the same tick on the same connection.

**Local API enforcement:**
- Attempting to enqueue the 65th command for the same tick MUST return `Result::Err`
- This is user-initiated misuse (per `00_common.spec.md`)

**Remote enforcement:**
- If a receiver observes `sequence >= 64`, it MUST treat it as invalid remote input
- The command MUST be dropped (no panic, per `00_common.spec.md`)
- Valid commands with `sequence < 64` for the same tick MUST still be processed normally

**Observable signals:**
- API returns `Err` when cap exceeded locally
- Commands with `sequence >= 64` are not applied

**Test obligations:**
- `commands-03c.t1`: Enqueueing 65th command returns `Err`
- `commands-03c.t2`: Received `sequence >= 64` is dropped without panic
- `commands-03c.t3`: Valid commands are unaffected by invalid sequence in same tick

---

### [commands-03d] — Duplicate command handling

**Duplicate detection:**
If two commands arrive with the same `(tick, sequence)` for a connection:
- The first received command is applied
- The later duplicate(s) MUST be dropped (treated as retransmit duplicates)
- MUST NOT panic (remote/untrusted input, per `00_common.spec.md`)
- MUST NOT re-apply the command

**Observable signals:**
- Command handler invoked exactly once per `(tick, sequence)`

**Test obligations:**
- `commands-03d.t1`: Duplicate `(tick, sequence)` commands are dropped
- `commands-03d.t2`: First-received duplicate wins

### [commands-04] — Client lead targeting is the primary mechanism to avoid late commands
The intended mechanism to ensure commands arrive on-time is client lead targeting (time-11/time-12). The server remains authoritative and will ignore late commands regardless.

### [commands-05] — Disconnect cleans in-flight command state
On disconnect:
- any buffered/in-flight commands for that session MUST be discarded,
- no commands from that session may be applied after disconnect.

---

## Test obligations

Summary of test obligations from contracts above:

**Time & Ticks:**
- `time-04.t1`: All durations use monotonic time provider
- `time-05.t1`: Deterministic time provider yields deterministic tick progression
- `time-07.t1`: Server tick advances exactly as implied by elapsed time and TickRate
- `time-09.t1`: Wrap-safe ordering holds across wrap boundary
- `time-09.t2`: Half-range tie is deterministic and does not corrupt ordering
- `time-10.t1`: ConnectEvent only after tick sync complete
- `time-11.t1`: Client lead converges toward target_lead
- `time-12.t1`: Client pacing adjusts to maintain lead

**Commands:**
- `commands-01.t1`: Every command is tagged to a tick
- `commands-02.t1`: Duplicate command deliveries do not double-apply
- `commands-03.t1`: On-time command is processed
- `commands-03.t2`: Late command is ignored
- `commands-03a.t1`: Every command includes a valid `sequence` value
- `commands-03b.t1`: Reordered packets still apply commands in sequence order
- `commands-03b.t2`: Commands are applied in send order regardless of arrival order
- `commands-03c.t1`: Enqueueing 65th command returns `Err`
- `commands-03c.t2`: Received `sequence >= 64` is dropped without panic
- `commands-03c.t3`: Valid commands are unaffected by invalid sequence in same tick
- `commands-03d.t1`: Duplicate `(tick, sequence)` commands are dropped
- `commands-03d.t2`: First-received duplicate wins
- `commands-05.t1`: Disconnect prevents any further command application
