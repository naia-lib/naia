# Transport Contract

This document defines the **only** valid semantics for transport behavior, fault model, packet delivery properties, fragmentation/reassembly, compression, and transport parity expectations.  
Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHALL**, **SHOULD**.

---

## Glossary

- **Transport**: The mechanism carrying Naia packets between client and server (e.g., UDP, WebRTC, local test transport).
- **Packet**: A single transport datagram/frame delivered (or not) by the underlying transport.
- **Delivery**: A packet arriving at the receiver (possibly delayed, duplicated, or reordered).
- **Loss**: A packet that never arrives.
- **Duplication**: The same packet arriving more than once.
- **Reordering**: Packets arriving in an order different from send order.
- **Jitter**: Variability in delivery latency.
- **MTU**: Maximum payload size for a single packet/frame without fragmentation.
- **Fragment**: A piece of a larger logical payload split across packets.
- **Reassembly**: Reconstructing a full logical payload from fragments.
- **Fault model**: The set of adverse network behaviors assumed possible (loss/duplication/reordering/jitter).
- **Parity**: “Same observable semantics” when running the same scenario over different transports, modulo timing.

---

## Contracts

### transport-01 — Fault model assumptions

**Rule:** The transport layer **MAY** exhibit loss, duplication, reordering, and jitter. Naia’s externally observable semantics **MUST** remain correct under this fault model, within the guarantees explicitly provided by channel/reliability semantics defined elsewhere.

**Notes:**
- This spec does not redefine channel semantics; it defines what the transport may do and what Naia must do in response.

**Test obligations:**
- TODO: scenario with controlled loss/dup/reorder/jitter proving no panics and stable convergence where semantics promise convergence.

---

### transport-02 — No duplicate observable events due to packet duplication

**Rule:** If the transport duplicates packets, Naia **MUST NOT** surface duplicate externally observable events that are defined to be one-shot (e.g., spawn/despawn, component insert/remove/update events, message delivery on reliable channels, request/response completion, authority events).

**Clarifications:**
- “Duplicate packet” includes duplicate fragments and duplicate whole packets.
- This rule is about API-observable duplication, not internal de-dup bookkeeping.

**Test obligations:**
- TODO: duplicate-heavy link conditioner run where spawns/updates/messages are not duplicated at API level.

---

### transport-03 — No regression under reordering

**Rule:** Under packet reordering, Naia **MUST NOT** regress observable state in ways forbidden by higher-level contracts (e.g., applying older state after newer state when sequencing rules forbid it; observing updates before spawn; observing events after despawn).

**Test obligations:**
- TODO: reorder scenario where older updates arrive late and are ignored when they would violate ordering/identity contracts.

---

### transport-04 — Deterministic test transport behavior

**Rule:** The test transport used for E2E tests **SHOULD** support deterministic control of fault injection (loss/dup/reorder/jitter) such that the same seed/config produces the same externally observable outcomes.

**Test obligations:**
- TODO: run the same scenario twice under identical fault injection settings and assert identical outcomes.

---

### transport-05 — MTU boundary behavior (no silent corruption)

**Rule:** If a logical payload exceeds MTU, Naia **MUST** either:
1) Fragment and reassemble correctly, OR
2) Fail cleanly with a defined error behavior (no panic, no corrupted partial application).

Naia **MUST NOT** apply partially received oversized payloads as if complete.

**Test obligations:**
- TODO: oversized update/message that exceeds MTU; assert either correct reassembly or clean failure, never partial state.

---

### transport-06 — Fragment reassembly completeness

**Rule:** When fragmentation is used, a receiver **MUST** apply the logical payload only after all required fragments have been received and validated. Missing fragments **MUST** prevent application of that payload.

**Test obligations:**
- TODO: drop one fragment of an oversized update; assert receiver stays at prior valid state until a complete later update arrives.

---

### transport-07 — Fragment duplication and reordering safety

**Rule:** Fragment duplication and reordering **MUST NOT** cause:
- duplicate application of the same logical payload, or
- partial/incorrect reassembly, or
- panics/assertions.

**Test obligations:**
- TODO: reorder + duplicate fragments under stress; assert exactly-once application (where applicable) and no partial state.

---

### transport-08 — Fragment lifetime and cleanup

**Rule:** Reassembly buffers **MUST** be bounded and **MUST** be cleaned up when:
- the reassembly completes, OR
- the payload becomes impossible to complete (e.g., exceeds time/window limits), OR
- the connection is closed.

Naia **MUST NOT** leak memory/state due to incomplete fragment sets.

**Test obligations:**
- TODO: long run with repeated incomplete fragment sets; assert bounded buffers and no growth/leaks.

---

### transport-09 — Compression toggles do not change semantics

**Rule:** Enabling or disabling compression **MUST NOT** change externally observable semantics (events, ordering, final replicated state, request/response results). Compression **MAY** change bandwidth/bytes-on-wire and timing.

**Test obligations:**
- TODO: run identical scenario with compression off/on; assert identical logical outcomes.

---

### transport-10 — Compression failure handling

**Rule:** If compression/decompression fails for a packet/payload, Naia **MUST** handle it safely:
- **MUST NOT** panic,
- **MUST NOT** apply corrupted partial state,
- **SHOULD** surface a defined error outcome (drop packet, disconnect, or error event) consistent with the system’s broader error model.

**Test obligations:**
- TODO: inject malformed compressed payload; assert safe failure path and clean state.

---

### transport-11 — Transport parity for core replication + messaging scenarios

**Rule:** For a defined set of “core scenarios” (spawn/update/despawn + representative messages/requests), running the same scenario over supported transports (e.g., UDP vs WebRTC) **MUST** produce identical externally observable semantics, modulo timing differences.

**Scope:**
- “Identical semantics” means same sequence of logical events and same final converged world state, allowing differences in timestamps and delivery latency.

**Test obligations:**
- TODO: run parity suite over UDP and WebRTC; compare event traces normalized for timing.

---

### transport-12 — Transport-specific connect failures are clean

**Rule:** If a transport-specific connection setup fails (e.g., WebRTC ICE/signaling), Naia **MUST**:
- surface a clear failure outcome,
- **MUST NOT** leave half-connected server state (no ghost user, no lingering scoped entities),
- **MUST** clean up resources and allow subsequent reconnection attempts.

**Test obligations:**
- TODO: forced WebRTC setup failure; assert no connect event and full cleanup.

---

### transport-13 — Ordering metadata integrity

**Rule:** Any internal ordering metadata used to enforce higher-level semantics (e.g., ordered IDs / sequence numbers / tick tags) **MUST** be validated and applied such that:
- old/duplicate deliveries do not cause repeated application,
- wraparound (if applicable) does not break monotonic comparisons within the supported window.

**Test obligations:**
- TODO: stress ordering metadata near wrap boundaries (if present) and assert correct ordering enforcement.

---

### transport-14 — “Kitchen sink” transport stress does not violate other contracts

**Rule:** Under a combined stress profile (moderate loss + jitter + duplication + reordering), Naia **MUST** continue to satisfy the contracts defined by:
- `entity_replication.md`
- `entity_scopes.md`
- `messaging.md`
- `time_ticks_commands.md`
- `entity_authority.md` / `entity_delegation.md` (when applicable)

This spec does not restate those rules; it requires they remain true under the fault model.

**Test obligations:**
- TODO: integrated scenario mixing replication, scope changes, messages, requests, and authority actions under link conditioner.

---

## Cross-References

- Replication semantics, spawn/update/despawn ordering, identity: `specs/entity_replication.md`
- Scope/snapshot rules: `specs/entity_scopes.md`
- Messaging channels and request/response semantics: `specs/messaging.md`
- Tick/time-based semantics and tick-buffered channels (if any): `specs/time_ticks_commands.md`
- Delegation/authority semantics under transport stress: `specs/entity_delegation.md`, `specs/entity_authority.md`

---
