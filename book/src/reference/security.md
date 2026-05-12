# Security & Trust Model

naia is a networking library, not a security framework. Understanding its trust
boundaries is essential before deploying in production.

---

## The server is authoritative

All persistent game state originates on the server. The server decides which
entities exist, which components they carry, and which values are canonical.
Clients receive a read-only view of the entities the server places in their scope.

## Authority delegation is bounded

When the server marks an entity `Delegated` and a client requests authority, the
server explicitly **grants or denies** the request. While a client holds authority
its mutations travel back to the server. The server can revoke authority at any
time by calling `entity_take_authority`. Clients never hold unrevocable ownership.

> **Danger:** naia replicates what the client sends — it does not validate or clamp values.
> Mutations from a client-authoritative entity **must** be validated server-side
> before being applied to authoritative game state.

## Authentication

naia supports application-layer authentication via a typed `Auth` message sent
during the handshake. Define any `#[derive(Message)]` struct as your auth
payload:

```rust
// shared/src/messages/auth.rs
#[derive(Message)]
pub struct Auth {
    pub username: String,
    pub token:    String,
}
```

The client sends it before the connection is accepted:

```rust
client.connect_with_auth(
    NativeSocket::new("127.0.0.1:14191"),
    &Auth { username: "alice".into(), token: jwt_token },
);
```

The server receives it via an `AuthEvent` (Bevy: `AuthEvents`) before the
connection is fully established:

```rust
for (user_key, auth) in auth_events.read::<Auth>() {
    if validate_token(&auth.token) {
        server.accept_connection(&user_key);
    } else {
        server.reject_connection(&user_key);
    }
}
```

> **Danger:** Auth credentials are transmitted in plaintext over UDP. Use
> `transport_webrtc` (DTLS encrypted) or a TLS proxy in front of UDP for any
> deployment where credentials must be confidential.

---

## What naia does NOT provide

- **Packet authentication or encryption.** `AuthEvent` credentials are transmitted
  in plaintext by default over UDP. Applications requiring confidentiality **must**
  choose an encrypted transport.
- **Anti-cheat.** naia does not detect or reject malicious client mutations.
- **Rate limiting.** naia does not throttle message or mutation rates at the
  application layer.
- **Input validation.** naia does not validate or sanitise component values received
  from client-authoritative entities.

---

## Transport encryption by deployment target

| Transport | Encryption | Suitable for |
|-----------|-----------|--------------|
| `transport_udp` (native) | **None — plaintext** | Local dev / trusted LAN only |
| `transport_webrtc` (browser) | DTLS (from WebRTC spec) | Internet browser clients |
| `transport_quic` (native, planned) | TLS 1.3 (Quinn) | Production native deployments |

**Production recommendation:** for native clients on untrusted networks, use
`transport_quic` once available. Until then, place `transport_udp` behind a VPN
or a TLS terminating proxy (e.g. stunnel, NGINX stream proxy) if confidentiality
is required.

---

## Securing native UDP deployments today

### stunnel configuration

Install stunnel (`apt install stunnel4` / `brew install stunnel`), then create
`/etc/stunnel/naia.conf`:

```ini
[naia-udp]
accept  = 0.0.0.0:14192          ; TLS port clients connect to
connect = 127.0.0.1:14191        ; naia server's plain UDP port
cert    = /etc/ssl/certs/naia.crt
key     = /etc/ssl/private/naia.key
protocol = connect
```

### docker-compose example

```yaml
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

---

## WebRTC (browser) considerations

Browser clients connect over WebRTC data channels. The WebRTC handshake provides
DTLS encryption at the transport layer, but the `AuthEvent` payload is still
application-layer plaintext from naia's perspective. If you transmit sensitive
credentials in `auth()`, ensure the WebRTC transport is configured for
end-to-end encryption.

---

## Reporting a vulnerability

Please report security issues privately to the maintainers via Discord or email
before filing a public GitHub issue.
