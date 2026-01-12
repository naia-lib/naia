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
- transport drop/dup/reorder assumptions (see `2_transport.md`)
- message channel ordering/reliability (see `3_messaging.md`)
- entity replication/lifetime (see entity suite)
- connection admission/auth steps (see `1_connection_lifecycle.md`)

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
A successful connection handshake MUST include tick synchronization, and the client MUST NOT emit `ConnectEvent` until tick sync is complete. (See `1_connection_lifecycle.md`.)

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

### [commands-03] — “Arrives in time” acceptance rule
A command tagged for tick `T` is considered on-time iff it is received by the server before the server begins processing tick `T`.

- If received on-time, the server MAY apply it when processing tick `T` (exact ordering among multiple commands for the same tick is implementation-defined, but MUST be deterministic).
- If received late (server has already begun or completed processing tick `T`), the server MUST ignore it.

Ignored late commands are remote/untrusted input outcomes:
- Prod: ignore silently
- Debug: ignore with warning

(There is no public “rejected command error” surfaced to the client; the contract is that late commands are ignored.)

### [commands-04] — Client lead targeting is the primary mechanism to avoid late commands
The intended mechanism to ensure commands arrive on-time is client lead targeting (time-11/time-12). The server remains authoritative and will ignore late commands regardless.

### [commands-05] — Disconnect cleans in-flight command state
On disconnect:
- any buffered/in-flight commands for that session MUST be discarded,
- no commands from that session may be applied after disconnect.

---

## Test obligations (TODO)

- time-04/time-05: deterministic time provider yields deterministic tick progression
- time-07: server tick advances exactly as implied by elapsed time and TickRate (no invented ticks)
- time-09: wrap-safe ordering holds across wrap; half-range tie is deterministic and does not corrupt ordering logic
- time-10: ConnectEvent only after tick sync complete
- time-11/time-12: client lead converges toward target_lead under changing RTT/jitter estimates
- commands-02: duplicate command deliveries do not double-apply
- commands-03: late commands are ignored deterministically
- commands-05: disconnect prevents any further command application
