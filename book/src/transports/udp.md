# Native UDP

**Crate:** `naia-socket-native` (exposed as `naia_server::transport::udp` and
`naia_client::transport::udp`)

`transport_udp` is naia's native socket implementation for Linux, macOS, and
Windows. It uses standard UDP datagrams and has no encryption.

---

## Setup

```rust
use naia_server::transport::udp::NativeSocket;

// Server:
server.listen(NativeSocket::new("0.0.0.0:14191"));

// Client:
client.connect(NativeSocket::new("127.0.0.1:14191"));
```

An optional `LinkConditionerConfig` can be passed to simulate packet loss,
latency, and jitter in development:

```rust
server.listen(NativeSocket::new_with_conditioner(
    "0.0.0.0:14191",
    LinkConditionerConfig::average_condition(),
));
```

---

## Auth TCP

The UDP transport uses a separate TCP connection for the initial handshake
(authentication and protocol hash check). The TCP connection is closed after
the handshake completes; all subsequent traffic is pure UDP.

---

## Production security

`transport_udp` is plaintext. For production on untrusted networks, place the
server behind a TLS-terminating proxy (stunnel, NGINX stream proxy) until
`transport_quic` is available. See [Security & Trust Model](../reference/security.md).
