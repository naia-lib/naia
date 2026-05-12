# Scaling Considerations

naia is a single-process authority. Understanding its scaling envelope helps
you right-size your server hardware and plan for growth.

---

## What determines concurrency limits

- **Bandwidth** — each connected user consumes `target_bytes_per_sec` of
  outbound bandwidth. At the default 512 kbps per user, a 1 Gbps uplink
  supports ~2000 concurrent users if all users are actively receiving
  replication. Real workloads use far less due to priority-weighted allocation
  and scope filtering.
- **CPU** — the bottleneck is typically the send loop (priority sort +
  serialization). Profile with `cargo bench --bench iai` to measure
  instructions per tick.
- **Memory** — Historian snapshot buffers are the dominant memory consumer on
  servers with many entities. Use component-kind filtering to limit snapshot
  scope.

---

## Horizontal scaling

naia is a single-process authority with no built-in cross-process state sharing.
For games that need horizontal scaling, use zone sharding at the application
layer. See [Entity Replication — Multi-Server / Zone Architecture](../concepts/replication.md).

---

## Reference numbers (naia bench, native UDP)

From the `halo_btb_16v16` bench report (16 clients × 16 entities, 20 Hz, basic
position replication):

| Metric | Value |
|--------|-------|
| Idle client latency | ~44 µs |
| Per-client send cost | ~722 ns |
| P95 tick duration | < 1 ms |

These are best-case numbers on loopback with quantized types. Real workloads
with more components, higher entity counts, and real network will differ.

---

## Hosting recommendations

Based on naia's CCU / cost analysis (see `naia/_AGENTS/CAPACITY_ANALYSIS_2026-04-26.md`):

- **Hetzner dedicated** — 35× better CCU/$ ratio than typical cloud VMs.
- **Cloudflare** — appropriate for serving HTML/JS/WASM assets; **not** for
  naia data traffic (UDP routing overhead is not worth it at game-networking
  packet rates).
