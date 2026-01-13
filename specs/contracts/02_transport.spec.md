# Transport

Last updated: 2026-01-08

This spec defines the transport boundary contract for **Naia** (`naia_client` + `naia_server`).
It is **transport-agnostic**: Naia can run over UDP, WebRTC, or local channels. Naia assumes transports are unordered/unreliable and does not rely on stronger guarantees even if a transport happens to provide them.

Reliability, ordering, fragmentation, resend, and dedupe guarantees belong to `03_messaging.spec.md`.

---

## Scope

This spec owns:
- Naia’s assumptions about the transport layer
- Naia’s packet-size boundary (MTU) and Naia-level error behavior
- Naia’s behavior on malformed/oversize inbound packets at the boundary

This spec does **not** own:
- Socket-crate-specific behavior (`naia_client_socket`, `naia_server_socket`)
- Message reliability/ordering/fragmentation semantics (see `03_messaging.spec.md`)
- Entity replication semantics (see entity suite specs)
- Auth semantics (see `01_connection_lifecycle.spec.md`)

---

## Definitions

- **Transport adapter**: the implementation used by Naia to send/receive packets (UDP/WebRTC/local).
- **Packet**: a single datagram-like unit delivered by the transport adapter.
- **Packet payload**: the bytes Naia asks the transport adapter to send in one packet.
- **MTU_SIZE_BYTES**: the maximum packet payload allowed by Naia core, exposed via `naia_shared`.
- **Prod vs Debug**: Debug means `debug_assertions` enabled; Prod means disabled.

---

## Contracts

### [transport-01] — Naia assumes transport is unordered & unreliable

**Obligations:**
- **t1**: Naia assumes transport is unordered & unreliable works correctly
Naia MUST assume packets may be dropped, duplicated, and reordered, and MUST NOT rely on:
- in-order delivery
- exactly-once delivery
- guaranteed delivery

(UDP/WebRTC/local are all valid so long as Naia can treat them as such.)

---

### [transport-02] — MTU boundary is defined by `naia_shared::MTU_SIZE_BYTES`

**Obligations:**
- **t1**: MTU boundary is defined by `naia_shared::MTU_SIZE_BYTES` works correctly
Naia MUST treat `MTU_SIZE_BYTES` as the maximum size of a **single packet payload**.

Naia MUST NOT knowingly ask a transport adapter to send a packet payload larger than `MTU_SIZE_BYTES`.

---

### [transport-03] — Oversize outbound packet attempt returns `Err` at Naia layer

**Obligations:**
- **t1**: Oversize outbound packet attempt returns `Err` at Naia layer works correctly
If Naia is asked (directly or indirectly) to send data that would require an outbound packet payload larger than `MTU_SIZE_BYTES`, Naia MUST return `Result::Err` from the initiating Naia-layer API.

This is a Naia contract (even if a particular transport adapter would panic). Naia must validate before calling the adapter.

---

### [transport-04] — Malformed or oversize inbound packets are dropped

**Obligations:**
- **t1**: Malformed or oversize inbound packets are dropped works correctly
If Naia receives a packet that is:
- larger than `MTU_SIZE_BYTES`, or
- malformed / cannot be decoded at the packet boundary,

then:
- In **Prod**: Naia MUST drop it silently.
- In **Debug**: Naia MUST drop it and emit a warning.

(Exact warning text is not part of the contract.)

---

### [transport-05] — No transport-specific guarantees may leak upward

**Obligations:**
- **t1**: No transport-specific guarantees may leak upward works correctly
Naia’s higher layers (messaging/replication) MUST behave identically regardless of whether the underlying transport happens to be “better” (e.g. local channels).
Any guarantee stronger than transport-01 MUST be explicitly specified in `03_messaging.spec.md`, not inferred from the transport adapter.

---

## Test obligations (TODO)
- transport-01: Verify Naia tolerates reorder/drop/duplicate at packet boundary (via test transport / local conditioner).
- transport-03: Verify oversize outbound attempt returns Err (and does not panic).
- transport-04: Verify malformed inbound is dropped (warn only in Debug).