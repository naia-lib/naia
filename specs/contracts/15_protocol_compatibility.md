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

- **Protocol**: the user-defined message/component/channel registry built via `Protocol::builder()`.
- **Protocol identity**: a fingerprint derived from the Protocol's structure.
- **Compatible**: client and server Protocols have identical Protocol identity.
- **Incompatible**: Protocol identities differ.

---

## 3) Contracts

### [protocol-01] — Protocol identity inputs

Protocol identity MUST be derived from:
- Channel registry (kinds, directions, modes)
- Message type registry (type IDs, field schemas)
- Component type registry (type IDs, field schemas)
- Replicated field order and types within each component

Protocol identity SHOULD also include:
- Naia wire protocol version (major version changes break compatibility)

Protocol identity MAY include:
- Compression settings (if they affect wire format)

**Non-inputs:**
- Naia crate version (patch/minor versions are compatible)
- Application version strings (unless explicitly added to Protocol)

**Observable signals:**
- Protocol hash/fingerprint is queryable (implementation-defined format)

**Test obligations:**
- `protocol-01.t1`: Different channel registrations produce different Protocol identity
- `protocol-01.t2`: Different component schemas produce different Protocol identity
- `protocol-01.t3`: Same Protocol produces same identity across builds

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

### [protocol-03] — Mismatch rejection behavior

If client and server Protocols are incompatible:
- Server MUST explicitly reject the connection attempt
- Client MUST emit `RejectEvent` (not `DisconnectEvent`)
- Client MUST NOT emit `ConnectEvent`
- Rejection reason SHOULD indicate protocol mismatch (implementation-defined text)

Per `0_common.md`:
- This is a user-initiated configuration error, so rejection is appropriate
- No panic occurs; connection simply fails

**Observable signals:**
- `RejectEvent` on client
- Rejection event on server (if applicable)

**Test obligations:**
- `protocol-03.t1`: Incompatible client receives `RejectEvent`
- `protocol-03.t2`: Compatible client receives `ConnectEvent`

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
