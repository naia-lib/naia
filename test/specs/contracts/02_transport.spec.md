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
- **t1**: Naia assumes transport is unordered & unreliable.
Naia MUST assume packets may be dropped, duplicated, and reordered, and MUST NOT rely on:
- in-order delivery
- exactly-once delivery
- guaranteed delivery

(UDP/WebRTC/local are all valid so long as Naia can treat them as such.)

---

### [transport-02] — MTU boundary is defined by `naia_shared::MTU_SIZE_BYTES`

**Obligations:**
- **t1**: MTU boundary is defined by `naia_shared::MTU_SIZE_BYTES`.
Naia MUST treat `MTU_SIZE_BYTES` as the maximum size of a **single packet payload**.

Naia MUST NOT knowingly ask a transport adapter to send a packet payload larger than `MTU_SIZE_BYTES`.

---

### [transport-03] — Oversize outbound packet attempt returns `Err` at Naia layer

**Obligations:**
- **t1**: Oversize outbound packet attempt returns `Err` at Naia layer.
If Naia is asked (directly or indirectly) to send data that would require an outbound packet payload larger than `MTU_SIZE_BYTES`, Naia MUST return `Result::Err` from the initiating Naia-layer API.

This is a Naia contract (even if a particular transport adapter would panic). Naia must validate before calling the adapter.

---

### [transport-04] — Malformed or oversize inbound packets are dropped

**Obligations:**
- **t1**: Malformed or oversize inbound packets are dropped.
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
- **t1**: No transport-specific guarantees may leak upward.
Naia’s higher layers (messaging/replication) MUST behave identically regardless of whether the underlying transport happens to be “better” (e.g. local channels).
Any guarantee stronger than transport-01 MUST be explicitly specified in `03_messaging.spec.md`, not inferred from the transport adapter.

---

### [transport-06] — Retried attempts never share the wire with a stale one

**Obligations:**
- **t1**: Every transport sender MUST release its attempt's live connection on `shutdown`, idempotently, so a retried attempt dials fresh state.
A sender whose socket and address state are owned by its attempt alone (UDP: fresh socket and address cell per `Socket::new`, shared only between that attempt's sender and receiver) releases them by teardown-drop and documents the no-op; backends holding shared connection state MUST close it eagerly instead.

---

### [transport-07] — Native client socket runs on smol, never tokio

**Obligations:**
- **t1**: The native client socket (`naia-client-socket`, non-wasm backends) MUST run on smol. It MUST NOT construct or depend on a tokio runtime: spawning uses `smol::spawn` on the global executor, and all channels are runtime-agnostic (`async-channel`, `futures-channel`).
- **t2**: `webrtc-unreliable-client` is consumed as the published registry version (`0.6.0`, smol runtime, no tokio), never a vendor copy and never a live out-of-repo path (public-repo invariant, Connor 2026-09-17; Connor 2026-10-29: "Obviously A is the only way to go. I'm furious that we ever vendored `webrtc-unreliable-client`!!!"). See Removed / not wanted below.

---

## Test obligations (TODO)
- transport-01: Verify Naia tolerates reorder/drop/duplicate at packet boundary (via test transport / local conditioner).
- transport-03: Verify oversize outbound attempt returns Err (and does not panic).
- transport-04: Verify malformed inbound is dropped (warn only in Debug).
- transport-05: Verify higher layers carry no transport cfgs (`higher_layers_carry_no_transport_cfgs`: no `transport_udp`/`transport_local` cfg outside `transport/` + `handshake/` subtrees and the one module gate) and the same suites stay green across default and UDP builds.
- transport-06: Verify shutdown is idempotent and preserves pre-connect state (`shutdown_is_idempotent_and_preserves_finding`, UDP build).

---

## Removed / not wanted

The following was deliberately removed from this repo and MUST NOT be reintroduced without an explicit ruling overturning the one cited.

### sibling-path consumption of webrtc-unreliable-client

- **What:** `socket/client` consuming the provider through the ordinary
  relative sibling path (`../../../webrtc-unreliable-client`), restoring
  commit `21d82b98` (never pushed).
- **Why not:** trips the standing public-repo invariant (Connor 2026-09-17):
  sibling path deps must never reach naia `dev`/`main`. Demonstrated by the
  native `lint:public-paths` gate-leg on `21d82b98`
  (`socket/client/Cargo.toml:39 escapes repo`, R0 stops the climb).
- **Interim:** Kepler 2026-10-29 (pending Connor's final ruling) supersedes
  only the 7zv "no re-vendor" clause: the sibling pin `15f7f85` is consumed
  as the byte-exact vendor copy again, with zero local deltas. RESOLVED by
  Connor 2026-10-29 (nydus-ffl): the provider is published as registry
  `0.6.0` and consumed from the registry; see below.

### vendoring of webrtc-unreliable-client

- **What:** `socket/client` and `socket/server` consuming the provider
  through the byte-exact vendor copy under `vendor/webrtc-unreliable-client`
  (pin `022e9de`, refreshed per `vendor/README.md`, entry in
  `siblings.lock`, fallback root in `.clerc/impact.toml`).
- **Why not:** Connor 2026-10-29 (nydus-ffl): "Obviously A is the only way
  to go. I'm furious that we ever vendored `webrtc-unreliable-client`!!!"
  (A = publish to crates.io; naia depends on the published version; no
  vendor copy).