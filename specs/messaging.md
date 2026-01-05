# Messaging Contract

This document defines the **only** valid semantics for messaging in Naia: channels, ordering, reliability, duplication, request/response, timeouts, and disconnect behavior.  
Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHALL**, **SHOULD**.

Related specs:
- `transport.md` (fault model: loss/reorder/duplication/jitter; MTU/fragmentation/compression; parity)
- `time_ticks_commands.md` (tick semantics, tick-buffered delivery rules, monotonic tick)
- `entity_scopes.md` (room/scope targeting rules for recipients)
- `observability_metrics.md` (bandwidth/RTT metrics; not required for correctness)

---

## Glossary

- **Message**: a discrete application payload sent over a channel from one endpoint to another.
- **Channel**: a configured stream with specific delivery semantics (reliability + ordering model).
- **Reliable**: delivery is retried until acknowledged or declared failed by documented timeout/budget.
- **Unreliable**: best-effort delivery; messages MAY be dropped without retries.
- **Ordered**: receiver MUST surface messages in the sender’s order for that channel.
- **Unordered**: receiver MAY surface messages in any order.
- **Sequenced**: receiver MUST only surface the latest message per stream, dropping older ones once newer is observed.
- **Tick-buffered**: receiver buffers by tick and surfaces groups in tick order (subject to window/drop rules).
- **Duplicate packet**: transport delivers the same underlying packet more than once.
- **Late message**: a message that arrives after a newer message for the same ordering context.
- **Request/Response**: a paired interaction with a correlation identifier and a single terminal outcome.

---

## Contract IDs

### Messaging Channel Semantics

#### messaging-01 — Per-channel isolation
Messages on one channel MUST NOT affect ordering/visibility of messages on any other channel (except as explicitly documented by the API).
- This means: if Channel A is ordered, that guarantee applies only within Channel A.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::per_channel_ordering_is_isolated`

---

#### messaging-02 — Reliable delivery: exactly-once to API surface
For **reliable** channels, if a message is successfully delivered, the receiver MUST surface it **exactly once** to the public events/API, even under packet duplication.
- The implementation MUST deduplicate delivered messages.
- The contract does not require “exactly-once” across process crashes or restarts—only within a live session.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::reliable_ignores_duplicates`

---

#### messaging-03 — Unreliable delivery: best-effort
For **unreliable** channels, messages MAY be lost. The receiver MUST NOT synthesize missing messages, and the sender MUST NOT retry unless documented otherwise.
- Under zero loss, messages SHOULD arrive.
- Under configured loss, some messages SHOULD NOT arrive.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::unreliable_best_effort_semantics`

---

#### messaging-04 — Ordered reliable channel preserves send order
For an **ordered reliable** channel, the receiver MUST surface messages in the same order they were sent on that channel, even under packet reordering and jitter.
- If duplicates arrive, they MUST NOT appear twice.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::ordered_reliable_preserves_order_under_reordering`

---

#### messaging-05 — Unordered reliable delivers all (but order not guaranteed)
For an **unordered reliable** channel, the receiver MUST surface all successfully delivered messages **exactly once**, but MAY surface them in any order.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::unordered_reliable_delivers_all_in_any_order`

---

#### messaging-06 — Sequenced reliable surfaces only the latest state
For a **sequenced reliable** channel used as a “latest state” stream:
- The receiver MAY drop older states.
- Once the receiver surfaces state `S_k`, it MUST NOT later surface any earlier state `S_j` where `j < k`.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::sequenced_reliable_latest_only_no_regress`

---

#### messaging-07 — Sequenced unreliable drops late/outdated updates
For a **sequenced unreliable** channel:
- The receiver MUST drop messages that are older than the latest observed sequence.
- The receiver MUST NOT regress to older states.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::sequenced_unreliable_drops_late_updates`

---

#### messaging-08 — Tick-buffered groups and surfaces by tick
For a **tick-buffered** channel:
- Messages tagged with tick `T` MUST be surfaced as part of group `T`.
- The receiver MUST NOT surface any message for tick `T+1` before it has surfaced all messages for tick `T` that are eligible to be surfaced.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::tick_buffered_groups_by_tick_and_preserves_tick_order`

---

#### messaging-09 — Tick-buffered drop window for old ticks
For a **tick-buffered** channel with a finite window:
- Messages for ticks older than the allowed window MUST be dropped and MUST NOT be surfaced.
- Dropping old ticks MUST NOT cause regressions of tick progression.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::tick_buffered_drops_too_old_ticks`

---

### Targeting & Recipient Semantics

#### messaging-10 — Recipient targeting respects scope/rooms
When the server targets a message to a set of recipients based on rooms/scope:
- Only clients that satisfy the server’s targeting predicate MUST receive the message.
- Non-targeted clients MUST NOT receive it.

**Test obligations**
- TODO: `test/tests/messaging_channels.rs::server_broadcast_respects_rooms_or_scope`

---

#### messaging-11 — No delivery to disconnected/rejected clients
If a connection is not in the connected state (disconnected/rejected/never-connected):
- Sending a message to that connection MUST fail predictably (error/ignored) and MUST NOT panic.
- The system MUST NOT resurrect the connection or create partial state.

**Test obligations**
- TODO: `test/tests/robustness_api_misuse.rs::sending_messages_on_disconnected_connection_is_safe`

---

### Request/Response Semantics

#### messaging-12 — Request produces exactly one terminal outcome
For a request `R` initiated by one side:
- The initiator MUST observe exactly one terminal outcome: **Success(response)** or **Failure(timeout/disconnect/error)**.
- Duplicate packets MUST NOT produce multiple responses delivered to the public API.

**Test obligations**
- TODO: `test/tests/request_response.rs::client_request_yields_exactly_one_response_or_failure`
- TODO: `test/tests/request_response.rs::server_request_yields_exactly_one_response_or_failure`

---

#### messaging-13 — Correlation identifiers are isolated per peer
Correlation identifiers MUST be scoped such that responses cannot be misrouted across clients.
- Multiple clients MAY reuse the same request id value without collision.
- The server MUST route responses to the correct originating client.

**Test obligations**
- TODO: `test/tests/request_response.rs::concurrent_requests_from_multiple_clients_are_isolated`

---

#### messaging-14 — Out-of-order completion is handled correctly
If requests complete in a different order than they were sent:
- The initiator MUST match each response to the correct request.
- The API MUST NOT swap or conflate responses.

**Test obligations**
- TODO: `test/tests/request_response.rs::out_of_order_responses_match_correct_requests`

---

#### messaging-15 — Request timeout is surfaced and cleaned up
If a request does not receive a response within the documented timeout:
- The initiator MUST surface a timeout failure.
- The implementation MUST release tracking state and MUST NOT leak.
- Any late response arriving after timeout MUST be ignored (or surfaced only if the documented contract allows it; default is ignore).

**Test obligations**
- TODO: `test/tests/request_response.rs::request_timeouts_are_surfaceable_and_do_not_leak`
- TODO: `test/tests/request_response.rs::late_response_after_timeout_is_ignored`

---

#### messaging-16 — Disconnect mid-flight yields failure and cleanup
If a disconnect occurs while requests are in-flight:
- The initiator MUST surface failure for those requests.
- Both sides MUST clean up in-flight request state.
- Late responses after disconnect MUST be ignored and MUST NOT panic.

**Test obligations**
- TODO: `test/tests/request_response.rs::in_flight_requests_fail_cleanly_on_disconnect`

---

### Illegal / Misuse Cases

#### messaging-17 — Illegal channel usage fails predictably
If the caller attempts to send a message that violates channel constraints (size/type/unsupported channel):
- The operation MUST fail predictably (error/ignored) and MUST NOT panic.
- The connection MUST remain usable for valid traffic.

**Test obligations**
- TODO: `test/tests/robustness_api_misuse.rs::illegal_channel_usage_is_safe`

---

## Notes / Cross-Spec Dependencies (Non-normative)

- Ordering/reliability guarantees are defined here; the **fault model** (how loss/jitter/reorder are simulated and what conditions are expected) lives in `transport.md`.
- Tick-buffered semantics assume a monotonic tick concept defined in `time_ticks_commands.md`.
- Recipient targeting (rooms/scope) is specified in `entity_scopes.md`; this spec only asserts messaging respects the chosen targeting predicate.
