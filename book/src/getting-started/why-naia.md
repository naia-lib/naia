# Why naia?

naia occupies a specific niche in the Rust multiplayer networking ecosystem:
**server-authoritative entity replication that targets both native and browser
from a single codebase**, with a rich set of built-in primitives for prediction,
lag compensation, and bandwidth control.

---

## Library comparison

| | **naia** | **lightyear** | **renet** | **bevy_replicon** | **GGRS** |
|-|----------|--------------|-----------|------------------|---------|
| Entity replication | ✅ delta-compressed | ✅ | ❌ messages only | ✅ | ❌ |
| Browser / WASM client | ✅ WebRTC | ❌ | ❌ | ❌ | ❌ |
| ECS-agnostic | ✅ | ❌ Bevy-only | ✅ | ❌ Bevy-only | ✅ |
| Lag compensation (Historian) | ✅ built-in | ❌ | ❌ | ❌ | ❌ |
| Priority-weighted bandwidth | ✅ built-in | ❌ | ❌ | ❌ | ❌ |
| Client-side prediction | primitives | built-in | ❌ | ❌ | ✅ GGPO-style |
| Interest management | rooms + UserScope | rooms | ❌ | visibility filter | ❌ |
| Authority delegation | ✅ | ✅ | ❌ | ❌ | ❌ |
| P2P / NAT traversal | ❌ | ❌ | ❌ | ❌ | ✅ (via matchbox) |
| zstd compression | ✅ + dict training | ❌ | ❌ | ❌ | ❌ |
| smol / async-std | ✅ | ❌ (tokio) | ❌ (tokio) | ❌ (tokio) | n/a |

---

## When to choose naia

- You need **browser clients** (WebRTC WASM) without a separate codebase.
- You want **built-in lag compensation** without rolling your own snapshot buffer.
- You need **fine-grained bandwidth control** per entity per user.
- Your stack uses **smol / async-std** (naia has no tokio dependency).
- You want an **ECS-agnostic** core you can wrap for any game framework.

## When to choose something else

- **lightyear** — you are all-in on Bevy and want the prediction/interpolation
  framework built-in rather than building it yourself.
- **renet** — you only need reliable message passing, not entity replication.
- **GGRS + matchbox** — you are building a fighting game or any fixed-roster
  P2P deterministic rollback game.
- **bevy_replicon** — you want the simplest possible Bevy replication and don't
  need browser clients or lag compensation.

---

## Relationship to Tribes 2

naia's internal networking model follows the
[Tribes 2 Networking Model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf)
(GDC 2000). If you have read that paper, the concepts of ghosts, scoping, and
packet send queues map directly to naia's entities, rooms + UserScope, and
`send_all_packets`.
