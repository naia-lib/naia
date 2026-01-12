# Protocol Compatibility & Versioning

This spec defines how Naia determines **protocol compatibility** between client and server, and what happens when compatibility checks fail.

---

## 1) Scope

This spec owns:
- Protocol identity computation
- Compatibility check timing and behavior
- Mismatch rejection semantics

This spec does NOT own:
- Wire format details
- Compression/serialization choices
- Transport-level handshake (see `1_connection_lifecycle.md`)

---

## 2) Definitions

- **Protocol crate**: the shared Rust crate that defines the message/component/channel registry via `Protocol::builder()`.
- **Protocol crate identity**: a compiled fingerprint of the protocol crate (includes version + build hash or equivalent).
- **Compatible**: client and server are built against the **same compiled protocol crate identity**.
- **Incompatible**: protocol crate identities differ.

---

## 3) Contracts

### [protocol-01] — Protocol crate identity is REQUIRED to match

**Strict requirement:** Server and client MUST be built against the **same compiled version of the shared protocol crate**.

**Protocol crate identity MUST be derived from:**
- Compiled protocol crate version
- Protocol crate build hash (or equivalent deterministic fingerprint)
- Channel registry (kinds, directions, modes, registration order)
- Message type registry (type IDs, field schemas, registration order)
- Component type registry (type IDs, field schemas, registration order)
- Replicated field order and types within each component
- Naia wire protocol version

**Implementation flexibility:**
- The exact mechanism for computing protocol crate identity is implementation-defined
- Common approaches: compile-time hash, version + git hash, deterministic schema hash
- The identity MUST be stable across identical builds of the same protocol crate

**No partial compatibility:**
- There is NO extension negotiation
- There is NO partial compatibility mode
- Either the protocol crate identity matches exactly, or the connection is rejected

**Observable signals:**
- Protocol crate identity is queryable at runtime (implementation-defined format)

**Test obligations:**
- `protocol-01.t1`: Different channel registrations produce different protocol crate identity
- `protocol-01.t2`: Different component schemas produce different protocol crate identity
- `protocol-01.t3`: Same protocol crate produces same identity across builds

---

### [protocol-02] — Compatibility check timing

Protocol compatibility MUST be verified during the connection handshake, BEFORE:
- `ConnectEvent` is emitted
- Entity replication begins
- Messages are delivered

**Observable signals:**
- Connection attempt fails before `ConnectEvent` if incompatible

**Test obligations:**
- `protocol-02.t1`: Incompatible client/server fails during handshake

---

### [protocol-03] — Protocol crate identity mismatch rejection

If client and server protocol crate identities do not match:
- Server MUST explicitly reject the connection attempt
- Client MUST emit `RejectEvent` (not `DisconnectEvent`)
- Client MUST NOT emit `ConnectEvent`
- Rejection reason MUST indicate **protocol crate identity mismatch** (implementation-defined text, but must be distinguishable from other rejection reasons)

Per `0_common.md`:
- This is a user-initiated configuration error (wrong protocol crate version deployed)
- No panic occurs; connection simply fails with clear rejection

**Observable signals:**
- `RejectEvent` on client with protocol mismatch indication
- Server may log/emit rejection event (implementation-defined)

**Test obligations:**
- `protocol-03.t1`: Mismatched protocol crate identity causes `RejectEvent` on client
- `protocol-03.t2`: Matched protocol crate identity allows `ConnectEvent`

---

### [protocol-04] — What must match exactly

For compatibility, the following MUST match exactly between client and server:

| Aspect | Must Match |
|--------|------------|
| Channel count | Yes |
| Channel kinds (IDs) | Yes |
| Channel modes | Yes |
| Channel directions | Yes |
| Message type count | Yes |
| Message type IDs | Yes |
| Message field schemas | Yes |
| Component type count | Yes |
| Component type IDs | Yes |
| Component field schemas | Yes |
| Naia wire protocol major version | Yes |

Order of registration MAY affect type IDs; ensure consistent registration order.

**Observable signals:**
- Connection success or rejection

**Test obligations:**
- `protocol-04.t1`: Mismatched channel count causes rejection
- `protocol-04.t2`: Mismatched component schema causes rejection
- `protocol-04.t3`: Matched protocols connect successfully

---

### [protocol-05] — Version upgrade path

When Protocol changes require a breaking change:
- Old clients MUST be rejected by new servers (protocol-03)
- Old servers MUST reject new clients (protocol-03)
- There is no automatic migration or upgrade negotiation

If gradual rollout is needed, application layer MUST:
- Run parallel server versions, OR
- Use feature flags within the Protocol, OR
- Implement custom version negotiation above Naia layer

**Observable signals:**
- (Not externally observable; policy constraint)

**Test obligations:**
- `protocol-05.t1`: Breaking protocol change causes rejection

---

## Test obligations

Summary of test obligations from contracts above:
- `protocol-01.t1`: Different channel registrations produce different Protocol identity
- `protocol-01.t2`: Different component schemas produce different Protocol identity
- `protocol-01.t3`: Same Protocol produces same identity across builds
- `protocol-02.t1`: Incompatible client/server fails during handshake
- `protocol-03.t1`: Incompatible client receives `RejectEvent`
- `protocol-03.t2`: Compatible client receives `ConnectEvent`
- `protocol-04.t1`: Mismatched channel count causes rejection
- `protocol-04.t2`: Mismatched component schema causes rejection
- `protocol-04.t3`: Matched protocols connect successfully
- `protocol-05.t1`: Breaking protocol change causes rejection

---

## Cross-references

- Connection lifecycle: `1_connection_lifecycle.md`
- Error taxonomy: `0_common.md`
