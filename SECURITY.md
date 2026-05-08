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
  or integrity guarantees MUST wrap the transport in TLS/DTLS.
- **Anti-cheat.** naia does not detect or reject malicious client mutations.
  Validate all client-originated state server-side.
- **Rate limiting.** naia does not throttle message or mutation rates at the
  application layer. Implement rate limiting in your game logic if needed.
- **Input validation.** naia does not validate or sanitise component values
  received from client-authoritative entities.

### WebRTC (browser) considerations

Browser clients connect over WebRTC data channels. The WebRTC handshake
provides DTLS encryption at the transport layer, but the `AuthEvent` payload
is still application-layer plaintext from naia's perspective. If you transmit
sensitive credentials in `auth()`, ensure the WebRTC transport is configured
for end-to-end encryption.

## Reporting a vulnerability

Please report security issues privately to the maintainers via Discord or
email before filing a public GitHub issue.
