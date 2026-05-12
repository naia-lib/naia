# Scaling Considerations

naia is a single-process authority. Understanding its scaling envelope helps
you right-size your server hardware and plan for growth.

---

## What determines concurrency limits

- **Bandwidth** — each connected user consumes approximately `target_bytes_per_sec`
  of outbound bandwidth. At the default 512 kbps per user, a 1 Gbps uplink
  supports ~2 000 concurrent users if all users are actively receiving
  replication. Real workloads use far less due to priority-weighted allocation
  and scope filtering — entities outside a user's scope send nothing.
- **CPU** — the bottleneck is typically the send loop (priority sort +
  serialization per connected user). Profile with `cargo bench --bench iai -p naia_bench`
  to measure instruction counts per tick at your target entity and user counts.
- **Memory** — Historian snapshot buffers are the dominant memory consumer on
  servers with many entities. Use `enable_historian_filtered` to limit snapshots
  to the component types you actually query.

---

## Reducing per-user bandwidth cost

1. **Scope filtering** — entities outside a user's `UserScope` are never sent.
   A user seeing only 10% of the world's entities uses only ~10% of the bandwidth
   of a user seeing everything.
2. **Priority gain `0.0`** — entities with gain `0.0` are never selected by the
   send loop, even if they are in scope. Use this for temporarily invisible
   entities (fog of war, behind walls).
3. **Static entities** — map geometry sent once, never diff-tracked.
4. **Quantized numeric types** — `SignedVariableFloat` encodes near-zero per-tick
   deltas in 3–4 bits vs 32 bits for a bare `f32`.
5. **zstd compression** — reduces the wire size of fully-packed packets by
   20–40% (default dictionary) or 40–60% (trained dictionary).

---

## Horizontal scaling

naia is a single-process authority with no built-in cross-process state sharing.
For games that need horizontal scaling, use zone sharding at the application
layer:

```
Zone A server (naia process)          Zone B server (naia process)
  owns entities in region A             owns entities in region B
        │                                       │
        └───── coordination service ────────────┘
                 (entity hand-off, cross-zone messages, matchmaking)
```

When a player moves between zones:

1. Serialize the player's replicated component state on the source server.
2. Send it to the destination server via your coordination channel (Redis, gRPC,
   direct TCP — your choice).
3. Despawn the entity on the source server (the client receives `DespawnEntityEvent`).
4. Spawn the entity on the destination server and add the client to the new room.

See [Entity Replication — Multi-Server / Zone Architecture](../concepts/replication.md#multi-server--zone-architecture)
for more detail.

---

## Reference numbers (naia bench, native UDP)

From the `halo_btb_16v16` benchmark scenario (16 clients × 16 server entities,
20 Hz, basic position replication with quantized types):

| Metric | Value |
|--------|-------|
| Idle client latency | ~44 µs |
| Per-client send cost | ~722 ns |
| P95 tick duration | < 1 ms |

These are best-case loopback numbers. Real workloads with more components, larger
entity counts, and real network conditions will differ. Use the benchmark suite
as a relative baseline — run before and after any architectural change.

```sh
cargo bench -p naia_bench
cargo bench --bench iai -p naia_bench   # requires Valgrind (instruction counts)
```

---

## Hosting recommendations

- **Dedicated bare-metal** — offers the best CCU-per-dollar ratio. Providers like
  Hetzner, OVH, and Vultr bare-metal give you predictable CPU and network without
  cloud overhead.
- **Cloud VMs** — convenient for auto-scaling but higher cost per CCU. Works fine
  for development and low-volume production.
- **Cloudflare / CDN edge** — appropriate for serving HTML/JS/WASM assets. Do
  **not** route naia's UDP data traffic through a CDN — the overhead at game
  networking packet rates is not worth the cost.
- **Multiple servers** — run one naia process per match or zone; use a lightweight
  coordination layer (a matchmaker, a Redis pub/sub, or a simple TCP relay) for
  cross-process coordination. This is the recommended path for horizontal scaling.

> **Tip:** Profile your specific component layout and entity count before
> optimizing. The priority accumulator, scope filtering, and static entities
> typically close most of the gap between theoretical limits and real workloads
> before you need horizontal scaling.
