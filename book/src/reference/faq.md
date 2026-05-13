# FAQ

## Choosing naia

### Should I start with WebRTC or UDP?

Start with WebRTC. It supports native and browser clients and includes DTLS.
Use UDP for local development, trusted LANs, benchmarks, or custom-secured
deployments where plaintext is an intentional choice.

### Is naia Bevy-only?

No. The Bevy adapter is the recommended path for Bevy games, but the core
`naia-server`, `naia-client`, and `naia-shared` crates are ECS-agnostic.
Macroquad uses the core client directly with the `mquad` feature.

### Does naia do P2P rollback networking?

No. naia is built around a server-mediated replicated world. For deterministic
P2P rollback games, look at GGRS/matchbox-style stacks.

## Crates And Setup

### Which crates do I use for Bevy?

Use `naia-bevy-shared` in your shared crate, `naia-bevy-server` in the server,
and `naia-bevy-client` in the client. The Bevy server/client crates re-export
the common shared primitives used by app code.

### Is there a macroquad adapter crate?

No. Use `naia-client` with the `mquad` feature and implement/use a core world
wrapper. See `demos/macroquad/`.

### Why does my client get rejected on connect?

The most common cause is a protocol mismatch. Both sides must register the same
components, messages, requests, resources, channels, tick settings, and relevant
feature-gated types.

## Replication

### Why is my Bevy entity not replicating?

You must call `enable_replication()` on the entity. A Bevy entity with a
`Replicate` component is still local until naia is told to track it.

### What does `Publicity::Private` mean?

For a client-owned replicated entity, `Private` means it replicates to the
server but is not published to other clients. For a truly local-only object, do
not enable naia replication.

### Can clients spawn replicated entities?

Yes, if the protocol opts in with `enable_client_authoritative_entities()`.
Treat this as a trust boundary: validate client-originated spawns and mutations
on the server.

### Can resources be delegated?

Yes. Replicated resources are hidden one-component entities internally and can
be configured as delegated resources.

## Prediction And Time

### Does naia provide client-side prediction?

naia provides primitives: tick-buffered input, command history, local duplicate
helpers, tick synchronization, and correction hooks. You still write the actual
prediction/interpolation policy for your game.

### Does naia provide lag compensation?

Yes. The `Historian` stores per-tick snapshots so the server can evaluate events
against the world a client saw when it acted.

## Messages

### Do messages use `Property<T>`?

No. `Property<T>` is for replicated component fields. Messages are serialized in
full each time they are sent.

### Are `Request` and `Response` derives?

No. Derive `Message`, then implement the `Request` and `Response` marker traits.
The request's associated `Response` type tells naia how to register and route
the pair.
