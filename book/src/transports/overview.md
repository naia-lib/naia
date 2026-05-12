# Transports Overview

naia's transport layer is pluggable via the `Socket` trait. Three
implementations ship out of the box:

| Transport | Target | Encryption | Best for |
|-----------|--------|------------|---------|
| `transport_udp` | Native | **None** | Local dev, trusted LAN |
| `transport_webrtc` | Browser WASM | DTLS | Internet browser clients |
| `transport_local` | In-process | n/a | Unit tests, AI bots |

The `Server` and `Client` APIs are identical for all transports — only the
`Socket` value passed to `listen` / `connect` differs.

---

## When to use each transport

- **UDP** — during development on a local machine or private network. Fast to
  set up, no encryption overhead.
- **WebRTC** — whenever you need browser clients. DTLS encryption is provided
  by the WebRTC spec automatically.
- **Local** — in your test harness or when running server and client in the same
  process (e.g. headless bots, determinism checks).

> **Warning:** `transport_udp` is **plaintext**. Never use it on an untrusted public network
> without a TLS proxy. See [Security & Trust Model](../reference/security.md).

---

## Planned transport

- **`transport_quic`** — TLS 1.3 native transport via Quinn. XL effort, no
  set timeline. When available, this will be the recommended transport for
  production native deployments.
