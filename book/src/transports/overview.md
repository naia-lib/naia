# Transports Overview

naia's transport layer is selected with Cargo features and exposed through
`naia_server::transport::*`, `naia_client::transport::*`, and the matching Bevy
adapter re-exports.

| Feature | Modules | Targets | Encryption | Best for |
|---------|---------|---------|------------|----------|
| `transport_webrtc` | `transport::webrtc` | Native server, native clients, Wasm clients | DTLS | Production default; browser support; mixed native/browser populations |
| `transport_udp` | `transport::udp` | Native only | None | Local dev, trusted LANs, custom secured deployments |
| `transport_local` | `transport::local` | Same process | n/a | Tests, harnesses, bots |

Use `naia-server`, `naia-client`, or the Bevy adapter crates with the transport
feature you need; transport selection is feature/module based.

---

## Default Recommendation

Start with **WebRTC** unless you have a specific reason not to. It works for
native and browser clients, includes the WebRTC handshake and DTLS encryption,
and keeps one server path for both desktop and Wasm builds.

Use **UDP** when you intentionally want plaintext native datagrams: local
development, trusted networks, benchmarks, or a deployment where you are adding
security at another layer and understand the tradeoff.

Use **local** when the network is not the thing being tested. It is how you make
replication tests deterministic and pleasantly free of port conflicts.

---

## Common Shape

The server and client APIs are the same regardless of transport:

```rust
// Server
server.listen(socket);

// Client
client.connect(socket);
```

The socket value changes; your protocol, rooms, replicated components, messages,
authority, and prediction code do not.
