# Comparing naia to Alternatives

This comparison is intentionally scoped to libraries a Rust multiplayer game
developer is likely to evaluate near naia: lightyear, bevy_replicon, and
matchbox. Some tools operate at different layers, so treat this as a decision
guide, not a courtroom exhibit.

Updated 2026-05.

External docs checked during this pass:
[lightyear](https://github.com/cBournhonesque/lightyear),
[bevy_replicon](https://docs.rs/bevy_replicon),
and [matchbox_socket](https://docs.rs/matchbox_socket).

---

## Feature Matrix

| Capability | **naia** | **lightyear** | **bevy_replicon** | **matchbox** |
|------------|----------|---------------|-------------------|--------------|
| Entity replication | Yes, ECS-agnostic core + Bevy adapter | Yes, Bevy-focused | Yes, Bevy-focused | No |
| Native + browser clients | WebRTC transport supports both | Wasm supported via WebTransport path | Depends on chosen transport | WebRTC sockets for browser/native use cases |
| Bevy integration | Yes | Yes | Yes | Via ecosystem glue |
| Non-Bevy integration | Core API + custom world traits | Not the focus | Not the focus | Yes, socket-level |
| Server-authoritative model | Yes | Yes | Yes | No, lower-level/P2P-oriented |
| Client-authoritative entities | Yes, opt-in | Varies by model | Not a direct equivalent | n/a |
| Authority delegation | Entities and resources | Entity authority model | Not a primary feature | n/a |
| Lag compensation primitive | `Historian` | Not a direct built-in equivalent | Not a direct built-in equivalent | n/a |
| Priority bandwidth allocation | Per-entity/per-user gain | Not a direct equivalent | Not a direct equivalent | n/a |
| Replicated resources | Yes | Bevy resource patterns differ | Yes, Bevy resources | n/a |
| Compression | Optional zstd + dictionary training | Check current feature set | Transport-dependent | Transport/socket-level |

---

## naia vs lightyear

Both projects cover server-authoritative replication, authority, prediction
building blocks, and Bevy users. lightyear has a polished Bevy-first prediction
and interpolation framework. naia is stronger when you want:

- WebRTC as a built-in naia transport for both native and Wasm clients.
- An ECS-agnostic core that can support Bevy, macroquad, or a custom world.
- Explicit client-authoritative entity publication.
- Delegated replicated resources.
- Historian-based lag compensation as a library primitive.
- Priority-weighted bandwidth allocation.
- Optional zstd compression and dictionary training.

Choose lightyear when you want a Bevy-native stack with more prediction framework
provided for you. Choose naia when transport flexibility, authority flexibility,
and bandwidth/lag-compensation primitives matter more than having a batteries-
included interpolation layer.

---

## naia vs bevy_replicon

bevy_replicon is a narrower Bevy replication library. It can be a good fit when
you want straightforward Bevy state replication and prefer to bring your own
transport and higher-level networking policy.

naia brings more machinery:

- Built-in transports, with WebRTC as the recommended path.
- Rooms plus per-user scope.
- Client-owned entities and publication states.
- Delegated entities/resources.
- Historian, prediction primitives, and priority bandwidth.
- A non-Bevy core API.

That machinery is valuable for larger or more network-sensitive games. For a
small Bevy-only project, bevy_replicon may be less to learn.

---

## naia vs matchbox

matchbox is primarily a WebRTC socket/signaling toolkit, often used for P2P and
rollback architectures. It is closer to a transport/session layer than a full
entity replication library.

Use matchbox when you want WebRTC sockets and plan to build your own replication
or deterministic rollback layer. Use naia when you want replicated entities,
messages, authority, scopes, and bandwidth management above the transport.

They can also be complementary conceptually: matchbox-style tooling is a good
fit for P2P rollback games, while naia is built around a server-mediated world.
