# Bandwidth Budget Analysis

Understanding and tuning your game's bandwidth consumption is critical for
scaling to more concurrent users and supporting mobile clients.

---

## Running the benchmark suite

```sh
cargo bench -p naia_bench
```

The benchmark suite (`benches/`) includes `bench_protocol.rs` with realistic
position, velocity, and rotation component types using naia's quantized numeric
types. Run it before and after tuning to verify improvements.

---

## Reading connection stats in production

```rust
// Server: sample every N ticks
if tick % 60 == 0 {
    for user_key in server.user_keys() {
        if let Some(stats) = server.connection_stats(&user_key) {
            println!(
                "user={:?} rtt_p50={:.0}ms p99={:.0}ms loss={:.1}% out={:.1}kbps in={:.1}kbps",
                user_key,
                stats.rtt_p50_ms, stats.rtt_p99_ms,
                stats.packet_loss_pct * 100.0,
                stats.kbps_sent, stats.kbps_recv
            );
        }
    }
}
```

---

## Tuning checklist

1. **Use quantized numeric types** — replace `Property<f32>` with
   `Property<SignedVariableFloat<BITS, FRAC>>` for position/velocity.
   See [Delta Compression](../advanced/delta-compression.md).

2. **Use static entities** for map geometry — zero per-tick cost after initial
   scope entry. See [Entity Replication](../concepts/replication.md).

3. **Set entity priority gain** — entities the player can't see get `0.0` gain
   (never sent); player-owned entity gets `3.0` (replicated 3× more often).
   See [Priority-Weighted Bandwidth](../advanced/bandwidth.md).

4. **Enable zstd compression** — `CompressionMode::Default(3)` reduces wire
   size ~30% with minimal CPU overhead.
   See [zstd Compression](../advanced/compression.md).

5. **Train a dictionary** — reduces wire size a further 40–60% vs default zstd
   on typical game-state delta packets.

6. **Reduce tick rate for non-interactive entities** — use priority gain to
   effectively reduce the rate without a separate "slow replication" pathway.
