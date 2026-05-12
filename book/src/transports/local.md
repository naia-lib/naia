# Local (In-Process) Transport

`transport_local` runs the server and client in the same process with no real
network sockets. It is used by naia's test harness and is ideal for:

- **Unit tests** — deterministic, no port conflicts, no OS networking stack.
- **Headless AI bots** — simulate a client without a network round-trip.
- **Determinism checks** — compare local-transport and real-network results.

---

## Setup

```rust
use naia_server::transport::local::{LocalServerSocket, LocalClientSocket};

// Create a shared hub:
let hub = LocalSocketHub::new();

// Server side:
server.listen(LocalServerSocket::new(&hub));

// Client side (same process):
client.connect(LocalClientSocket::new(&hub));
```

---

## Link conditioning

The local transport supports the same `LinkConditionerConfig` as UDP:

```rust
hub.configure_link_conditioner(LinkConditionerConfig::poor_condition());
```

This injects loss, latency, and jitter into the in-process message delivery —
useful for testing your prediction and rollback logic without a real bad network.

> **Tip:** Use `transport_local` + `LinkConditionerConfig::poor_condition()` to stress-test
> your prediction/rollback handler before deploying on a real network.
