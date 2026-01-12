# Request/Response (RPC) Semantics

This spec defines the semantics for Naia's request/response messaging pattern, commonly used for RPC-style communication.

---

## 1) Scope

This spec owns:
- Request ID uniqueness and matching
- Timeout and cancellation on disconnect
- Deduplication under retransmit
- Ordering relative to other message types

This spec does NOT own:
- Channel configuration (see `3_messaging.md`)
- Transport reliability (see `2_transport.md`)
- Specific RPC patterns (application-level)

---

## 2) Definitions

- **Request**: a message sent with the expectation of a matching Response.
- **Response**: a message sent in reply to a specific Request.
- **Request ID**: a unique identifier pairing a Request with its Response.
- **Pending request**: a Request that has been sent but not yet matched with a Response or canceled.
- **RPC channel**: a channel configured to support request/response semantics.

---

## 3) Contracts

### [rpc-01] — Request ID uniqueness

Each Request MUST have a unique Request ID within the scope of:
- The sending endpoint (client or server)
- The lifetime of the connection

Request IDs MUST NOT be reused for different logical requests within the same connection. Implementation MAY use monotonic counters, UUIDs, or other unique identifiers.

**Observable signals:**
- Request ID is available on Request and Response messages

**Test obligations:**
- `rpc-01.t1`: Multiple requests have distinct IDs
- `rpc-01.t2`: Response correctly matches Request by ID

---

### [rpc-02] — Response matching

A Response MUST be matched to its Request by Request ID:
- The receiver MUST pair the Response with the pending Request having the same ID
- If no pending Request exists for the ID, the Response MUST be ignored (per `0_common.md` remote input rule)
- Each Request MUST receive at most one Response (first valid Response wins)

**Observable signals:**
- Response handler invoked with matching Request context

**Test obligations:**
- `rpc-02.t1`: Response is delivered to correct Request handler
- `rpc-02.t2`: Orphan Response (no matching Request) is dropped silently

---

### [rpc-03] — Timeout semantics

Pending requests MAY have an optional timeout:
- If a Response is not received within the timeout, the Request MUST be canceled locally
- Timeout is measured using Naia's monotonic time source (see `0_common.md`)
- Canceled Requests MUST invoke their error/timeout handler (if provided)
- Late Responses for timed-out Requests MUST be ignored

If no timeout is specified, Requests remain pending until:
- Response is received, OR
- Connection disconnects (rpc-04)

**Observable signals:**
- Timeout handler invoked after timeout elapses

**Test obligations:**
- `rpc-03.t1`: Request times out if no Response within timeout
- `rpc-03.t2`: Late Response after timeout is ignored

---

### [rpc-04] — Disconnect cancels pending requests

When a connection disconnects:
- All pending Requests on that connection MUST be canceled
- Pending Request handlers MUST be invoked with a disconnect/error indication
- No Responses from disconnected sessions may be delivered

This ensures cleanup and prevents resource leaks.

**Observable signals:**
- All pending Request handlers invoked with error on disconnect

**Test obligations:**
- `rpc-04.t1`: Pending requests canceled on disconnect
- `rpc-04.t2`: Request handlers receive error indication

---

### [rpc-05] — Deduplication under retransmit

If underlying transport causes Request or Response to be retransmitted:
- Duplicate Requests MUST NOT cause duplicate processing on receiver
- Duplicate Responses MUST NOT cause duplicate delivery to requester
- Deduplication MUST use Request ID (same ID = same logical request)

Per `3_messaging.md`, reliable channels already provide deduplication at the message level. This contract ensures RPC semantics are maintained.

**Observable signals:**
- Request handler invoked exactly once per logical Request
- Response handler invoked exactly once per logical Response

**Test obligations:**
- `rpc-05.t1`: Duplicate Request delivery does not duplicate processing
- `rpc-05.t2`: Duplicate Response delivery does not duplicate handling

---

### [rpc-06] — Ordering relative to other messages

Request/Response ordering follows the underlying channel's ordering guarantees:
- On OrderedReliable: Requests and Responses maintain send order
- On UnorderedReliable: Requests and Responses may arrive out of order relative to each other and to other messages
- On SequencedReliable: Latest-wins semantics apply

Request/Response ordering is independent of:
- Entity replication (no guaranteed ordering between RPC and replication)
- Other channel traffic (independent channels have independent ordering)

**Observable signals:**
- Message delivery order per channel semantics

**Test obligations:**
- `rpc-06.t1`: Ordered channel maintains Request/Response order
- `rpc-06.t2`: RPC and replication are independently ordered

---

### [rpc-07] — Request without Response (fire-and-forget)

If a Request is sent without registering a Response handler:
- The Response (if any) MUST be dropped
- This is valid usage for "fire-and-forget" patterns
- No timeout applies (request is not tracked as pending)

This is distinct from a Message (non-RPC); Requests always carry an ID even if unused.

**Observable signals:**
- Response is dropped (not an error)

**Test obligations:**
- `rpc-07.t1`: Fire-and-forget Request without Response handler works

---

## 4) Error Handling

Per `0_common.md`:
- Invalid Request ID from remote: drop silently (remote input)
- Timeout: invoke handler with error (expected condition)
- Disconnect: invoke handler with error (expected condition)
- Internal invariant violation (e.g., duplicate pending ID): panic (framework bug)

---

## Test obligations

Summary of test obligations from contracts above:
- `rpc-01.t1`: Multiple requests have distinct IDs
- `rpc-01.t2`: Response correctly matches Request by ID
- `rpc-02.t1`: Response is delivered to correct Request handler
- `rpc-02.t2`: Orphan Response is dropped silently
- `rpc-03.t1`: Request times out if no Response within timeout
- `rpc-03.t2`: Late Response after timeout is ignored
- `rpc-04.t1`: Pending requests canceled on disconnect
- `rpc-04.t2`: Request handlers receive error indication
- `rpc-05.t1`: Duplicate Request delivery does not duplicate processing
- `rpc-05.t2`: Duplicate Response delivery does not duplicate handling
- `rpc-06.t1`: Ordered channel maintains Request/Response order
- `rpc-06.t2`: RPC and replication are independently ordered
- `rpc-07.t1`: Fire-and-forget Request without Response handler works

---

## Cross-references

- Messaging channels: `3_messaging.md`
- Error taxonomy: `0_common.md`
- Connection lifecycle: `1_connection_lifecycle.md`
