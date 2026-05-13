# Writing a Custom Transport

naia's protocol logic talks to transport traits rather than directly to OS
sockets. A custom transport can be built by implementing the server-side and
client-side socket traits exposed from:

- `naia_server::transport::Socket`
- `naia_client::transport::Socket`

The built-in UDP, WebRTC, and local transports are the practical templates.

---

## What a Transport Must Provide

The server-side socket is consumed by `server.listen(socket)` and produces four
handles:

- auth sender
- auth receiver
- packet sender
- packet receiver

The client-side socket is consumed by `client.connect(socket)` and produces:

- identity receiver
- packet sender
- packet receiver

That split is important. naia has an authentication/identity phase before normal
packet exchange, so a transport is more than a single `send(bytes)` function.

---

## Best References

Start with the smallest built-ins:

- `server/src/transport/local/`
- `client/src/transport/local/`

Then compare the production transports:

- `server/src/transport/webrtc.rs`
- `client/src/transport/webrtc.rs`
- `server/src/transport/udp.rs`
- `client/src/transport/udp/`

Those implementations show how to adapt an underlying network backend into
naia's auth, identity, sender, and receiver handles.

---

## When to Write One

Custom transports are advanced. Reach for them when you need a network layer
naia does not ship, such as a platform service, a managed relay, or a proprietary
transport required by a console or storefront.

For most games, prefer `transport_webrtc` first.
