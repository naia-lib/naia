# Benchmarking

naia ships a benchmark suite using Criterion and iai-callgrind.

---

## Running benchmarks

```sh
# Criterion throughput benchmarks
cargo bench -p naia_bench

# iai-callgrind instruction-count benchmarks (requires Valgrind)
cargo bench --bench iai -p naia_bench
```

---

## What the bench suite covers

- **`bench_protocol`** — serialization and deserialization of typical game
  component types (`PositionQ`, `VelocityQ`, `RotationQ`) using quantized
  numeric types. This is the baseline for measuring wire-size improvements
  from tuning `Property<T>` types.
- **`bench_replication`** — a `halo_btb_16v16` scenario: 16 clients receiving
  replication from 16 server entities, measuring per-tick send throughput.
- **`bench_histogram`** — priority accumulator sorting and bandwidth allocation
  under various entity counts.

---

## Adding your own benchmarks

Add a file under `benches/src/` and register it in `benches/Cargo.toml`. Use
naia's quantized types and a realistic component layout to get numbers that
reflect real game workloads.

> **Tip:** Run benchmarks on the same hardware before and after a change. Criterion's
> output includes regression detection — a >5% regression on `bench_replication`
> is worth investigating before merging.
