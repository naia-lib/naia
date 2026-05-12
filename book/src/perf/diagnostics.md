# Connection Diagnostics

naia exposes connection health metrics via `ConnectionStats`. These are computed
on demand from internal ring buffers and cover the full picture of link quality.

---

## Available metrics

| Field | Description |
|-------|-------------|
| `rtt_ms` | Round-trip time EWMA in milliseconds |
| `rtt_p50_ms` | RTT 50th-percentile from the last 32 samples |
| `rtt_p99_ms` | RTT 99th-percentile from the last 32 samples |
| `jitter_ms` | EWMA of half the absolute RTT deviation |
| `packet_loss_pct` | Fraction of sent packets unacknowledged in the last 64-packet window (`0.0`–`1.0`) |
| `kbps_sent` | Rolling-average outgoing bandwidth in kilobits per second |
| `kbps_recv` | Rolling-average incoming bandwidth in kilobits per second |

---

## Sampling

```rust
// Server side — sample once per second (not every frame):
if let Some(stats) = server.connection_stats(&user_key) {
    // log or push to your metrics backend
}

// Client side:
let stats = client.connection_stats();
```

> **Warning:** `connection_stats` performs a small sort for the percentile computation. Call
> it at most once per frame per connection — not inside a hot inner loop.

---

## Interpreting the numbers

- **rtt_p99 > 300 ms** — players on this connection will feel prediction
  corrections. Consider widening `TickBufferSettings` and deepening
  `CommandHistory`.
- **packet_loss_pct > 0.02** (2%) — entity updates may be delayed or arrive
  out of order. Test your rollback handler with `LinkConditionerConfig::poor_condition()`.
- **kbps_sent near target_bytes_per_sec** — the entity list is bandwidth-limited.
  Use priority gain to prioritize the most important entities; consider enabling
  zstd compression.
