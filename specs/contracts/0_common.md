# Common Definitions and Policies

This spec defines cross-cutting concerns that apply to all Naia specification documents:
- Error handling taxonomy
- Determinism requirements
- Test conventions
- Configuration defaults vs invariants
- Observability policies

All other specs MUST reference this document for these concerns and MUST NOT contradict its policies.

---

## 1) Error Handling Taxonomy

This section defines the **canonical error handling rules** for all Naia specifications. All specs MUST follow this taxonomy.

### Error/Failure Mode Summary

| Condition | Response | Panic? |
|-----------|----------|--------|
| Public API misuse | Return `Result::Err` | No |
| Remote/untrusted input | Drop (optionally warn in debug) | No |
| Protocol mismatch | Reject with `ProtocolMismatch` | No |
| Framework invariant violation | Panic | Yes |

**Key principle:** Panic is reserved for internal invariant violations only. No user action via public API can trigger a panic.

---

### [common-01] — User-initiated misuse returns Result::Err

When an error is caused by **local application code** or **local configuration** at the Naia API layer, Naia MUST return `Result::Err` from the initiating API.

Examples:
- Invalid channel configuration
- Sending on a channel not configured for that direction
- Oversize message payload
- Authority request on non-delegated entity
- Write attempt to entity the caller doesn't have permission to write
- Removing a server-replicated component from an unowned entity
- Enqueueing more than `MAX_COMMANDS_PER_TICK_PER_CONNECTION` commands

This applies when the **caller can reasonably check preconditions** before calling.

**Rule:** If user code can trigger a condition via public API, that condition MUST NOT panic. It MUST return `Err` or be prevented by the API design.

---

### [common-02] — Remote/untrusted input MUST NOT panic

When an error is caused by **remote input** or **network behavior** (malformed payload, reordering, duplicates, stale ticks, unresolved entity references, late arrivals, spam), Naia MUST NOT panic.

**Production behavior:**
- Ignore/drop silently
- MAY increment a metric counter (non-normative)

**Debug behavior:**
- Ignore/drop with warning
- Warning text is not part of the contract

Examples:
- Malformed or oversize inbound packet
- Duplicate replication messages
- Authority request for out-of-scope entity (server-side)
- Late command for already-processed tick
- TickBuffered message for evicted/old tick
- TickBuffered message too far in the future
- EntityProperty referencing unknown entity
- Command with `sequence >= MAX_COMMANDS_PER_TICK_PER_CONNECTION`
- Invalid Request ID in response

---

### [common-02a] — Protocol mismatch is a deployment error

When `protocol_id` does not match between client and server (see `1_connection_lifecycle.md`):
- Connection MUST be rejected with `ProtocolMismatch` error/event
- Client MUST receive distinguishable `ProtocolMismatch` indication
- MUST NOT panic (this is a deployment configuration error, not a runtime error)

**Classification:** Protocol mismatch is neither user API misuse nor remote attack—it's a **deployment configuration error** (wrong client/server versions deployed together).

---

### [common-03] — Framework invariant violations MUST panic

If Naia violates an invariant stated in its specifications (a condition that should be unreachable in correct implementations), Naia MUST panic.

These are considered **Naia bugs** and are expected to be unreachable.

Examples:
- Tick goes backwards in public API (after wrap-safe comparison)
- Older state delivered after newer state on a sequenced channel
- Internal send exceeding declared bounds
- Internal write path attempts to replicate entity client doesn't own
- GlobalEntity counter rollover

**Key rule:** These panics are for **internal invariants only**. If user code via public API can trigger the condition, it MUST NOT panic—use `Result::Err` instead or prevent the condition via API design.

---

### [common-04] — Warnings are debug-only and non-normative

In Debug mode (when `debug_assertions` are enabled or equivalent feature flag), Naia MAY emit warnings for unusual but handled conditions.

**Rules:**
- Warning text and format are not part of the contract
- Tests MUST NOT assert on warning content or presence
- Warnings MUST NOT affect observable behavior
- Warnings MAY be used for debugging but not for correctness

---

## 2) Determinism Requirements

### [common-05] — Determinism under deterministic inputs

If all of the following are deterministic:
- Time Provider (test clock)
- Network input sequence
- Application API call sequence

Then Naia's observable outputs MUST be deterministic:
- Event emission order
- Entity spawn/despawn order
- Component insert/update/remove order
- Authority state transitions

This enables reproducible testing.

---

### [common-06] — Per-tick determinism rule

Within a single server tick, if multiple operations could occur in any order, Naia MUST define a deterministic resolution:

**Scope operations (include/exclude/room changes):**
- Last API call wins in server-thread call order within the tick
- Server collapses to final resolved state; no intermediate spawn/despawn

**Multiple commands for same tick:**
- Server processes in receipt order (first received, first processed)
- If received in same packet, process in serialization order

**Multiple authority requests for same entity:**
- First request received wins (see `10_entity_delegation.md`)

---

## 3) Test Conventions

### [common-07] — Tests MUST NOT assert on logs

Tests MUST NOT assert on:
- Log message content
- Log message presence
- Warning text
- Debug output format

If a spec requires observable behavior, it MUST define an event, API return value, or world state that tests can assert on. Logs are for human debugging only.

---

### [common-08] — Test obligation template

Every contract SHOULD have test obligations in this format:

```markdown
**Test obligations:**
- `<contract-id>.t1`: <What the test verifies>
- `<contract-id>.t2`: <What the test verifies>
```

Test names SHOULD follow the pattern `<contract-id>.t<N>` for traceability.

---

### [common-09] — Observable signals subsection

Every contract that defines testable behavior SHOULD include:

```markdown
**Observable signals:**
- <Event type> / <API return> / <World state change>
```

This section names the **externally observable** outcomes tests can assert on.

If behavior is intentionally not externally observable (internal optimization, silent drop), state:

```markdown
**Observable signals:**
- (Not externally observable; behavior is internal)
```

---

## 4) Configuration: Defaults vs Invariants

### [common-10] — Fixed invariants are locked

Some values are **fixed invariants** that MUST NOT be configurable:

| Invariant | Value | Rationale | Spec |
|-----------|-------|-----------|------|
| `MAX_RELIABLE_MESSAGE_FRAGMENTS` | 2^16 | Protocol limit | `3_messaging.md` |
| `GlobalEntity` rollover behavior | Panic | Correctness over availability | `7_entity_replication.md` |
| Tick type | u16 | Wire protocol | `4_time_ticks_commands.md` |
| Wrap-safe half-range | 32768 | Tick ordering math | `4_time_ticks_commands.md` |
| Request ID uniqueness scope | Per-connection | RPC semantics | `3_messaging.md` |
| `MAX_COMMANDS_PER_TICK_PER_CONNECTION` | 64 | Command cap per tick | `4_time_ticks_commands.md` |
| `protocol_id` wire encoding | u128 little-endian | Protocol identity | `1_connection_lifecycle.md` |
| Command `sequence` encoding | varint | Wire protocol | `4_time_ticks_commands.md` |

These values are part of the protocol identity and/or correctness requirements. Changing them would break compatibility or violate safety invariants.

---

### [common-11] — Configurable defaults

Some values are **configurable defaults** that MAY be overridden via configuration:

| Default | Value | Config Location | Spec |
|---------|-------|-----------------|------|
| Identity token TTL | 1 hour | ServerConfig | `1_connection_lifecycle.md` |
| `ENTITY_PROPERTY_RESOLUTION_TTL` | 60 seconds | SharedConfig | `3_messaging.md` |
| `MAX_PENDING_ENTITY_PROPERTY_MESSAGES_PER_CONNECTION` | 4096 | SharedConfig | `3_messaging.md` |
| `MAX_PENDING_ENTITY_PROPERTY_MESSAGES_PER_ENTITY` | 128 | SharedConfig | `3_messaging.md` |
| TickBuffered `tick_buffer_capacity` | Per-channel | ChannelConfig | `3_messaging.md` |
| `MAX_FUTURE_TICKS` | Derived from `tick_buffer_capacity - 1` | Automatic | `3_messaging.md` |
| Tick rate | Per-protocol | SharedConfig | `4_time_ticks_commands.md` |
| `DEFAULT_REQUEST_TIMEOUT` | 30 seconds | SharedConfig | `3_messaging.md` |

**Compatibility rule:** When configurable values differ between client and server (where applicable), the more restrictive value MUST be used for safety, or connection MUST fail if incompatible.

---

### [common-11a] — New constants start as invariants

Any **new constant** introduced by this spec suite MUST be written as an **invariant initially** (with exact value documented).

**Policy:**
- New constants MAY be promoted to configurable later with proper versioning
- The spec MUST note when a constant becomes configurable
- This prevents accidental reliance on flexibility that doesn't exist yet

**Existing reality rule:**
- If Naia already exposes a value as config → spec MUST describe it as config
- If Naia already treats a value as invariant → spec MUST keep it invariant
- Specs MUST NOT claim configurability that doesn't exist in implementation

---

### [common-12a] — Test tolerance constants

These constants define acceptable tolerances for E2E test assertions:

| Constant | Value | Description |
|----------|-------|-------------|
| `RTT_TOLERANCE_PERCENT` | 20 | Acceptable deviation from expected RTT |
| `RTT_MIN_SAMPLES` | 10 | Minimum samples before asserting RTT convergence |
| `RTT_MAX_VALUE_MS` | 10000 | Maximum valid RTT (sanity bound) |
| `THROUGHPUT_TOLERANCE_PERCENT` | 15 | Acceptable deviation from expected throughput |
| `THROUGHPUT_MIN_SAMPLES` | 5 | Minimum samples before asserting throughput |
| `LEAD_CONVERGENCE_TICKS` | 60 | Ticks to allow client tick lead to stabilize |
| `METRIC_WINDOW_DURATION_MS` | 1000 | Default metric aggregation window |

These are test-only values and do not affect runtime behavior.

---

## 5) Observability Policies

### [common-12] — Internal measurements vs exposed metrics

Naia uses internal measurements (RTT, jitter, bandwidth) for:
- Client tick lead targeting
- Pacing decisions
- Internal timeouts

**Rule:** Reading observability metrics (via public API) MUST NOT influence internal behavior. Metrics are read-only observations of internal state.

**Rule:** Internal measurements MAY differ in precision/timing from exposed metrics. Metrics are for monitoring, not gameplay.

---

### [common-13] — Metrics are non-normative for gameplay

Observability metrics (RTT, throughput, etc.) MUST NOT affect:
- Replicated state correctness
- Authority decisions
- Scope decisions
- Message delivery semantics

Tests SHOULD NOT depend on exact metric values for correctness testing. Metric tests verify the metrics API itself, not gameplay behavior.

---

## 6) Connection Semantics

### [common-14] — Reconnect is fresh session

When a client "reconnects" (disconnects and connects again):
- This is a **fresh connection** that builds world state from a new snapshot
- Session resumption is **out of scope** unless explicitly specified
- The server treats the reconnecting client as a new session
- Any prior entity state, authority, buffered data is discarded

Rationale: Simplifies implementation and ensures clean state.

---

## Test obligations

The contracts in this document are cross-cutting policies. They are tested indirectly through domain-specific specs, but the following direct tests apply:

**Error Handling:**
- `common-01.t1`: API misuse returns `Err`, not panic
- `common-02.t1`: Remote/untrusted input is dropped without panic
- `common-02a.t1`: Protocol mismatch produces `ProtocolMismatch` error, not panic
- `common-03.t1`: Internal invariant violation panics (framework test only)

**Determinism:**
- `common-05.t1`: Identical inputs produce identical outputs under deterministic time
- `common-06.t1`: Same-tick operations resolve deterministically

**Test Conventions:**
- `common-07.t1`: No test asserts on log content (policy check)

**Observability:**
- `common-12.t1`: Reading metrics does not influence internal behavior
- `common-13.t1`: Metric values do not affect replicated state

**Connection:**
- `common-14.t1`: Reconnect builds fresh state, not resumed state

---

## Cross-references

This document is referenced by all specs in `specs/contracts/`.

Specs that define error handling MUST cite this document for the taxonomy.
