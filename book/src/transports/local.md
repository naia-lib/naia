# Local (In-Process) Transport

`transport_local` runs the server and client in the same process with no real
network sockets. It is used by naia's test harness and is ideal for:

- **Unit tests** — deterministic, no port conflicts, no OS networking stack.
- **Headless AI bots** — simulate a client without a network round-trip.
- **Determinism checks** — compare local-transport and real-network results.

---

## Setup

The local transport is mostly a test-harness tool. The pieces are:

- `naia_shared::transport::local::LocalTransportHub`
- `naia_server::transport::local::{LocalServerSocket, Socket}`
- `naia_client::transport::local::{LocalClientSocket, LocalAddrCell, Socket}`

The repository's contract harness and Bevy resource tests are the best reference
for complete setup because they wire the hub, auth queues, and data queues
directly. See `test/harness/src/harness/scenario.rs` and
`adapters/bevy/server/tests/replicated_resources_bevy.rs`.

---

## Link conditioning

The local transport supports `LinkConditionerConfig`:

Pass `Some(LinkConditionerConfig::poor_condition())` to the local client/server
socket wrapper to inject loss, latency, and jitter into in-process delivery.

This injects loss, latency, and jitter into the in-process message delivery —
useful for testing your prediction and rollback logic without a real bad network.

> **Tip:** Use `transport_local` + `LinkConditionerConfig::poor_condition()` to stress-test
> your prediction/rollback handler before deploying on a real network.
