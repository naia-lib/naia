# Security & Trust Model

naia is a networking library, not an anti-cheat or identity platform. It gives
you transport choices, typed auth payloads, authority boundaries, and the hooks
to validate client-originated state. Your game still owns trust decisions.

---

## Prefer WebRTC For Production

`transport_webrtc` is the recommended starting point for internet-facing games.
It works for native and browser clients and gets DTLS from WebRTC. Use it unless
you have a concrete reason to choose plaintext UDP.

`transport_udp` sends auth and game packets in plaintext. It is appropriate for
local development, trusted LANs, controlled benchmarks, or teams intentionally
wrapping/securing it themselves.

| Transport | Encryption | Recommended use |
|-----------|------------|-----------------|
| `transport_webrtc` | DTLS | Production native/browser clients |
| `transport_udp` | None | Local dev, trusted LANs, explicit custom security |
| `transport_local` | n/a | Same-process tests |

---

## Authority Boundaries

Server-owned undelegated entities are only written by the server. Clients affect
them by sending input/messages, and the server decides what state changes.

Client-authoritative entities are opt-in through
`Protocol::enable_client_authoritative_entities()`. Once enabled, client-owned
entities can replicate to the server and, if public, to other scoped clients.

Delegated entities/resources are server-owned, but a client may temporarily hold
write authority after the server grants it. The server can revoke authority.

> **Danger:** naia replicates client-originated values; it does not decide
> whether those values are fair. Validate positions, inventory changes, cooldowns,
> purchases, and every other client-originated mutation before making it game
> truth.

---

## Authentication

naia supports application-layer authentication via a typed `Message` sent during
the handshake:

```rust
use naia_bevy_shared::Message;

#[derive(Message)]
pub struct Auth {
    pub username: String,
    pub token: String,
}
```

Client:

```rust
client.auth(Auth {
    username: "alice".into(),
    token: jwt_token,
});
client.connect(socket);
```

Server:

```rust
for events in auth_events.read() {
    for (user_key, auth) in events.read::<Auth>() {
        if validate_token(&auth.token) {
            server.accept_connection(&user_key);
        } else {
            server.reject_connection(&user_key);
        }
    }
}
```

When credentials matter, use WebRTC or another encrypted deployment path. Do not
send secrets over plaintext UDP and then act surprised when plaintext behaves
like plaintext.

---

## What naia Does Not Provide

- Anti-cheat decisions.
- Rate limiting for application-level spam.
- Password/session-token storage.
- Protection against malicious but protocol-valid component values.
- P2P trust negotiation.

Those belong in your game server and infrastructure.
