# Messaging

Last updated: 2026-01-08

This spec defines Naia’s **message channel** contract for `naia_client` + `naia_server`.

It owns:
- Channel registration & configuration semantics (direction, mode)
- Delivery/ordering/duplication guarantees per ChannelMode
- Fragmentation rules for reliable channels
- Rules for messages containing `EntityProperty` (entity references) and entity-lifetime safety
- Buffering bounds & TTLs required for determinism + memory safety

It does NOT own:
- Transport adapter behavior (see `2_transport.md`)
- Entity replication semantics (see entity suite specs)
- Connection/auth handshake rules (see `1_connection_lifecycle.md`)

---

## Definitions

- **Channel**: A configured lane used to send/receive Messages (and optionally Requests/Responses).
- **ChannelKind**: A unique identifier for a channel type in a Protocol.
- **ChannelDirection**: The allowed send direction for a channel (Client→Server or Server→Client, as configured).
- **ChannelMode**: The delivery/ordering semantics of a channel. Naia exposes multiple modes.
- **Reliable**: Naia guarantees eventual delivery of a message while the connection remains active, and ensures the application observes the message at most once (deduped).
- **Ordered**: Application observes messages in the same order they were sent on that channel.
- **Sequenced**: Messages represent “current state”; older state MUST NOT be observed after newer state has been observed (no rollback). Intermediate states MAY be skipped.
- **TickBuffered**: Messages are grouped by tick and exposed per tick in tick order.
- **Tick**: A shared tick value used by Naia; `Tick` is `u16` and wraps.
- **Entity lifetime** (client-side): scope enter → scope leave, with the “≥ 1 tick out-of-scope” rule (see `6_entity_scopes.md` / `7_entity_replication.md`).

Normative keywords: MUST, MUST NOT, MAY, SHOULD.

---

## Global error-handling policy

### [messaging-01] — User-initiated errors are Results
When an error is caused by local application code or local configuration (e.g. invalid channel configuration, oversize payload send), Naia MUST return `Result::Err` from the initiating API rather than panicking.

### [messaging-02] — Remote/untrusted input MUST NOT panic
When an error is caused by remote input or the network (malformed payload, reorder, duplicates, stale ticks, unresolved entity references, spam), Naia MUST NOT panic.
- In Prod: drop silently
- In Debug: drop and emit a warning (exact text not specified)

### [messaging-03] — Framework invariant violations MUST panic
If Naia violates its own declared invariants (e.g. delivers older state after newer on a sequenced channel, attempts internal send exceeding declared bounds), Naia MUST panic.

(These conditions are considered Naia bugs and are expected to be unreachable in correct implementations.)

---

## Channel configuration

### [messaging-04] — Protocol crate identity determines channel compatibility

Server and client MUST be built against the **same compiled version of the shared protocol crate** that defines the channel registry.

**Protocol crate identity requirement:**
- The connection handshake MUST include a protocol crate identity value (see `15_protocol_compatibility.md`)
- If the protocol crate identity differs between client and server, the connection MUST be rejected

**Consequence of identity match:**
- Since the protocol crate identity MUST match, ChannelKind mapping MAY be derived from registration order in that crate
- Same ChannelKind refers to the same logical channel (guaranteed by identical protocol crate)
- ChannelMode and ChannelDirection are guaranteed compatible (same compiled protocol)

**Observable signals:**
- Connection rejected with protocol mismatch error if identities differ

**Test obligations:**
- `messaging-04.t1`: Mismatched protocol crate identity causes connection rejection
- `messaging-04.t2`: Matched protocol crate allows successful connection

### [messaging-05] — ChannelDirection is enforced at send-time
If local code attempts to send a message on a channel that is not configured for that direction, Naia MUST return `Result::Err`. (user-initiated)

---

## ChannelMode guarantee matrix

This table defines the observable application-level contract.

| ChannelMode | Delivery | Dedup | Ordering | Sequenced “no rollback” |
|---|---|---|---|---|
| UnorderedUnreliable | best-effort (may drop) | no | none | no |
| SequencedUnreliable | best-effort (may drop) | no | none | YES |
| UnorderedReliable | eventual while connected | YES | none | no |
| OrderedReliable | eventual while connected | YES | YES (send order) | no |
| SequencedReliable | eventual while connected (latest) | YES | none | YES |
| TickBuffered | per tick buffer (Client→Server only) | (mode-defined; see below) | tick order | n/a |

---

## UnorderedUnreliable

### [messaging-06] — Best-effort, no ordering, duplicates allowed
UnorderedUnreliable:
- MAY drop messages
- MAY deliver messages out of send order
- MAY deliver duplicates (application must tolerate)

---

## SequencedUnreliable

### [messaging-07] — Best-effort, “latest wins”, no rollback
SequencedUnreliable:
- MAY drop messages
- MAY deliver out of send order
- MUST enforce sequenced semantics:
    - Once the application has observed message M with sequence S_new, it MUST NOT later observe any message with sequence S_old where S_old is older than S_new (wrapping-safe comparison required).
    - Intermediate sequence values MAY be skipped.

Duplicates MAY occur (unreliable), and MUST NOT cause rollback.

---

## UnorderedReliable

### [messaging-08] — Reliable delivery, deduped, unordered
UnorderedReliable:
- MUST ensure eventual delivery while the connection remains active
- MUST dedupe so each message is observed at most once
- MUST NOT guarantee send-order delivery

---

## OrderedReliable

### [messaging-09] — Reliable + strict send-order delivery
OrderedReliable:
- MUST ensure eventual delivery while connected
- MUST dedupe so each message is observed at most once
- MUST deliver messages to the application in the same order they were sent on that channel
- MUST use wrap-safe ordering/indices to preserve correctness across wrap-around

---

## SequencedReliable

### [messaging-10] — Reliable + “latest wins” + no rollback
SequencedReliable is intended for “current-state streams”.

SequencedReliable:
- MUST ensure eventual delivery of the newest state while connected
- MUST dedupe (at-most-once observation for any given delivered state)
- MUST enforce sequenced semantics:
    - Once the application has observed a message with sequence S_new, it MUST NOT later observe any message with sequence older than S_new.
    - Intermediate states MAY be skipped.
- MUST NOT allow a receiver to revert to an older state due to reordering, retransmission, or delayed delivery.

---

## TickBuffered

TickBuffered is a standalone ChannelMode with TickBufferSettings.

### [messaging-11] — TickBuffered is Client→Server only
TickBuffered channels MUST be configurable only for Client→Server direction.
If configured for any other direction, Naia MUST return `Result::Err`. (user-initiated)

### [messaging-12] — TickBuffered groups messages by tick and exposes ticks in order
TickBuffered:
- Each message is associated with a Tick.
- The receiver MUST buffer messages grouped by Tick.
- When the receiver exposes buffered messages, it MUST expose ticks in increasing tick order (wrap-safe).
- A tick MAY have zero, one, or many messages.

### [messaging-13] — TickBuffered capacity and eviction
TickBuffered has a configurable `tick_buffer_capacity` (number of ticks that can be buffered).
- The receiver MUST NOT retain messages for more than `tick_buffer_capacity` distinct ticks.
- If adding a message for a new tick would exceed capacity, the receiver MUST evict the **oldest buffered tick groups first** (oldest ticks, in wrap-safe order) until within capacity.
- Eviction is considered remote/untrusted pressure; Naia MUST NOT panic. (See messaging-02)

**Eviction policy:** Always evict oldest tick first (FIFO by tick order).

### [messaging-14] — TickBuffered discards very-late ticks
If a message arrives for a tick that is older than the oldest tick currently retained (i.e., it would fall behind the retained window), the receiver MUST discard it.
- Prod: discard silently
- Debug: discard with warning (non-normative; tests MUST NOT assert on warning)

**Observable signals:**
- Message is not delivered (no handler invocation)
- (Debug only) Warning may be emitted

**Test obligations:**
- `messaging-14.t1`: Very-late tick message is not delivered

---

### [messaging-15-a] — TickBuffered discards too-far-ahead ticks

If a TickBuffered message arrives with tick > `current_server_tick + MAX_FUTURE_TICKS`, it MUST be dropped (no processing, no panic).

**Derived bound:**
- `MAX_FUTURE_TICKS = tick_buffer_capacity - 1`
- This bound is derived from the configured `tick_buffer_capacity` for the channel

**Rationale:** The future bound is tied to capacity because:
1. Messages for ticks beyond the buffer capacity would immediately cause eviction
2. This prevents clients from sending messages tagged with arbitrarily far-future ticks
3. The bound ensures the buffer window is predictable and memory-bounded

**Error handling (per `0_common.md`):**
- Prod: drop silently
- Debug: drop with warning (non-normative)
- MUST NOT panic (remote/untrusted input)

**Observable signals:**
- Message is not delivered (no handler invocation)
- (Debug only) Warning may be emitted

**Test obligations:**
- `messaging-15-a.t1`: Too-far-ahead tick message is dropped silently
- `messaging-15-a.t2`: Message at `current_tick + MAX_FUTURE_TICKS` is accepted
- `messaging-15-a.t3`: Message at `current_tick + MAX_FUTURE_TICKS + 1` is dropped

---

## Fragmentation and MTU

Naia defines a maximum packet payload size `MTU_SIZE_BYTES` at the transport boundary.

### [messaging-15] — Unreliable channels MUST NOT fragment
For UnorderedUnreliable and SequencedUnreliable:
- If a message payload would require fragmentation, Naia MUST return `Result::Err` from the send call. (user-initiated)

### [messaging-16] — Reliable channels MAY fragment up to a hard bound
For UnorderedReliable / OrderedReliable / SequencedReliable:
- Naia MAY fragment a message across multiple packets.
- Maximum fragments per message is a fixed bound:

  `MAX_RELIABLE_MESSAGE_FRAGMENTS = 2^16`

- If a user attempts to send a message requiring more than the bound, Naia MUST return `Result::Err`. (user-initiated)
- If Naia internally attempts to exceed this bound, Naia MUST panic. (framework invariant)

---

## Wrap-around safety

Tick and (where applicable) channel indices/sequence numbers wrap and must be compared using wrap-safe logic. Naia provides explicit wrapping helpers in shared code.

### [messaging-17] — Wrap-around MUST NOT break ordering or sequencing contracts
All ordering/sequence comparisons (OrderedReliable ordering, Sequenced* “newer than” checks, TickBuffered tick ordering) MUST be correct across wrap-around.

---

## Messages containing EntityProperty

Messages may contain EntityProperty values which refer to entities that may or may not currently exist in the receiver's active entity lifetime.

### [messaging-18] — EntityProperty resolution policy: buffer until mapped

A message that contains an EntityProperty MUST NOT be applied to an entity outside its current active lifetime.

**Default resolution policy (buffer until mapped):**
If the entity mapping is not yet known on receipt, the client MUST buffer the EntityProperty message until:
1. **The entity becomes mapped** (entity spawn is received and processed) → then apply the message, OR
2. **The `ENTITY_PROPERTY_RESOLUTION_TTL` expires** → then drop the message

**Lifetime safety:**
- Naia MUST NOT apply a buffered EntityProperty message after the referenced entity has completed a lifetime and despawned (no cross-lifetime leakage).
- If an entity despawns while messages are buffered for it, those buffered messages MUST be dropped.

**Observable signals:**
- Tests can deliver EntityProperty before spawn and still expect eventual application after spawn within TTL

**Test obligations:**
- `messaging-18.t1`: EntityProperty received before spawn is applied after spawn
- `messaging-18.t2`: EntityProperty for despawned entity is never applied

### [messaging-19] — EntityProperty resolution TTL (bounded buffering by time)
Naia MUST enforce a TTL on buffered EntityProperty messages:

`ENTITY_PROPERTY_RESOLUTION_TTL = 60 seconds` (configurable default)

- The TTL MUST be measured using Naia's monotonic time source (not wall-clock time).
- A buffered message that remains unresolved longer than TTL MUST be dropped.
  - Prod: drop silently
  - Debug: drop with warning (non-normative)
- TTL expiry is remote/untrusted input pressure; Naia MUST NOT panic.

**Determinism requirement:**
- Under a deterministic time source (test clock), identical scripted time advancement MUST produce identical TTL drop behavior.

**Test obligations:**
- `messaging-19.t1`: Buffered EntityProperty dropped after TTL expires

### [messaging-20] — EntityProperty buffering hard cap
In addition to TTL, Naia MUST enforce a hard cap to prevent unbounded memory growth:

- `MAX_PENDING_ENTITY_PROPERTY_MESSAGES_PER_CONNECTION = 4096`
- `MAX_PENDING_ENTITY_PROPERTY_MESSAGES_PER_ENTITY = 128`

**Eviction policy:** When cap would be exceeded, evict **oldest messages first** (FIFO).

If the cap would be exceeded:
- Naia MUST drop buffered messages using oldest-first eviction until within cap.
- Prod: silent
- Debug: warning (non-normative)
- MUST NOT panic

**Test obligations:**
- `messaging-20.t1`: Buffer cap enforced with oldest-first eviction

---

## Test obligations (TODO)

- messaging-06: UnorderedUnreliable can reorder/drop/duplicate; receiver does not assume otherwise.
- messaging-07: SequencedUnreliable never rolls back after newer state is observed.
- messaging-08: UnorderedReliable dedupes and eventually delivers while connected.
- messaging-09: OrderedReliable delivers in send order despite network reorder.
- messaging-10: SequencedReliable exposes only latest; never rolls back.
- messaging-11..14: TickBuffered grouping, order, capacity eviction, very-late tick discard.
- messaging-15: Unreliable oversize send returns Err (no fragmenting).
- messaging-16: Reliable fragmentation works up to 2^16 fragments; oversize returns Err.
- messaging-18..20: EntityProperty buffering obeys TTL + cap; never leaks across lifetimes.
- messaging-17: Wrap-around does not break any above guarantees.
