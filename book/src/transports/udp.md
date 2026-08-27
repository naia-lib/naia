# Native UDP

`transport_udp` is naia's native plaintext UDP transport. It is useful for local
development, trusted networks, benchmarks, and deployments where you deliberately
provide your own security layer. It should not be the default choice for
internet-facing games.

```toml
naia-server = { version = "0.26", features = ["transport_udp"] }
naia-client = { version = "0.26", features = ["transport_udp"] }
```

The transport is exposed as `naia_server::transport::udp` and
`naia_client::transport::udp`.

---

## Server Setup

```rust
use naia_server::transport::udp;

let addrs = udp::ServerAddrs::new(
    "0.0.0.0:14191".parse().unwrap(), // auth TCP
    "0.0.0.0:14192".parse().unwrap(), // UDP data
    "http://127.0.0.1:14192",         // public UDP URL
);
let socket = udp::Socket::new(&addrs, None);
server.listen(socket);
```

The UDP transport uses a TCP auth handshake and then sends game traffic over
UDP. Both auth payloads and game packets are plaintext.

---

## Client Setup

```rust
use naia_client::transport::udp;

let socket = udp::Socket::new("http://127.0.0.1:14191", client.socket_config());
client.connect(socket);
```

---

## Link Conditioning

UDP sockets accept an optional `LinkConditionerConfig` for latency, jitter, and
loss simulation:

```rust
use naia_shared::LinkConditionerConfig;

let lag = LinkConditionerConfig::average_condition();
let socket = udp::Socket::new(&addrs, Some(lag));
server.listen(socket);
```

Prefer [WebRTC](webrtc.md) for production unless you are intentionally accepting
the plaintext UDP tradeoff.

---

## Client and Server Builds Must Agree on `transport_udp`

Enabling `transport_udp` turns on source-address validation: an HMAC
challenge/validate round-trip prepended to the handshake, which proves a client
can actually receive at the address it claims before the server commits any
per-connection state to it. Raw UDP needs this because forging a source address
costs an attacker nothing; WebRTC does not, because ICE and DTLS already
establish it.

The handshake header is serialized by positional variant index, so the two
builds do not share a wire format for the handshake. A client built without
`transport_udp` cannot complete a handshake against a server built with it, or
vice versa — the connection simply never establishes. Build both sides with the
same setting.
