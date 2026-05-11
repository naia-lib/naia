# Security Policy

## Trust model

naia is a networking library, not a security framework. Understanding its
trust boundaries is essential before deploying in production.

### The server is authoritative

All persistent game state originates on the server. The server decides which
entities exist, which components they carry, and which values are canonical.
Clients receive a read-only view of the entities the server places in their
scope.

### Authority delegation is bounded

When the server marks an entity `Delegated` and a client requests authority,
the server explicitly **grants or denies** the request. While a client holds
authority its mutations travel back to the server. The server can revoke
authority at any time by calling `entity_take_authority`. Clients never hold
unrevocable ownership.

**Application responsibility:** mutations from a client-authoritative entity
should be validated server-side before being applied to authoritative game
state. naia replicates what the client sends — it does not validate or clamp
values.

### What naia does NOT provide

- **Packet authentication or encryption.** `AuthEvent` credentials are
  transmitted in plaintext by default. Applications that require confidentiality
  or integrity guarantees MUST choose an encrypted transport (see below).
- **Anti-cheat.** naia does not detect or reject malicious client mutations.
  Validate all client-originated state server-side.
- **Rate limiting.** naia does not throttle message or mutation rates at the
  application layer. Implement rate limiting in your game logic if needed.
- **Input validation.** naia does not validate or sanitise component values
  received from client-authoritative entities.

### Transport encryption by deployment target

| Transport | Encryption | Suitable for |
|-----------|-----------|--------------|
| `transport_udp` (native) | **None — plaintext** | Local dev / trusted LAN only |
| `transport_webrtc` (browser) | DTLS (from WebRTC spec) | Internet browser clients |
| `transport_quic` (native, planned) | TLS 1.3 (Quinn) | Production native deployments |

**Production recommendation:** for native clients on untrusted networks, use
`transport_quic` once available — it provides TLS 1.3 with no additional
configuration. Until then, place `transport_udp` behind a VPN or a TLS
terminating proxy (e.g. stunnel, NGINX stream proxy) if confidentiality is
required.

### WebRTC (browser) considerations

Browser clients connect over WebRTC data channels. The WebRTC handshake
provides DTLS encryption at the transport layer, but the `AuthEvent` payload
is still application-layer plaintext from naia's perspective. If you transmit
sensitive credentials in `auth()`, ensure the WebRTC transport is configured
for end-to-end encryption.

## Securing native UDP deployments today

Until `transport_quic` is available, the recommended path for native clients
on untrusted networks is a **TLS-terminating proxy** on the server side.
Below is a minimal working setup using stunnel.

### stunnel configuration

Install stunnel (`apt install stunnel4` / `brew install stunnel`), then create
`/etc/stunnel/naia.conf`:

```ini
; /etc/stunnel/naia.conf
[naia-udp]
accept  = 0.0.0.0:14192          ; TLS port clients connect to
connect = 127.0.0.1:14191        ; naia server's plain UDP port
; cert and key from Let's Encrypt or self-signed:
cert    = /etc/ssl/certs/naia.crt
key     = /etc/ssl/private/naia.key
protocol = connect
```

Run with `stunnel /etc/stunnel/naia.conf`. Clients connect to port 14192 via
TLS; stunnel unwraps TLS and forwards UDP to port 14191 where naia listens.

### docker-compose example

```yaml
# docker-compose.yml
services:
  game-server:
    image: your-game-server:latest
    environment:
      LISTEN_ADDR: "0.0.0.0:14191"
    expose:
      - "14191"

  stunnel:
    image: dweomer/stunnel
    ports:
      - "14192:14192/udp"
    volumes:
      - ./stunnel.conf:/etc/stunnel/stunnel.conf:ro
      - ./certs:/etc/stunnel/certs:ro
    depends_on:
      - game-server
```

```ini
; stunnel.conf (mounted into container)
[naia]
accept  = 0.0.0.0:14192
connect = game-server:14191
cert    = /etc/stunnel/certs/naia.crt
key     = /etc/stunnel/certs/naia.key
```

### AEAD stepping stone (future)

A lighter-weight alternative to a full TLS proxy — a symmetric
**AEAD-over-UDP** mode using XChaCha20-Poly1305 — is under evaluation. This
would require a pre-shared key exchanged out-of-band (e.g. via HTTPS login
flow) but would close the confidentiality gap for most indie use cases without
requiring QUIC or a proxy process. No implementation timeline is set; the
stunnel path above is the production recommendation today.

## Reporting a vulnerability

Please report security issues privately to the maintainers via Discord or
email before filing a public GitHub issue.
