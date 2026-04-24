# Bench Suite Polish — Follow-up Work

Follow-up work items identified during the 2026-04-24 reflection on the
initial benchmark suite landing (commit `b7f2bd3d`). Items #1–#7 below map
to the reflection's numbered list. Item #5 (CI regression gate) was
explicitly deferred by Connor and is **not tracked here**.

Tasks are listed in the order they should be executed. Each task has a
unique ID matching the runtime task list so a resumed session can map
back; IDs are stable only within a single session — the **order and
description** are the durable record.

---

## Status Key

- [ ] pending
- [~] in progress
- [x] complete

---

## Task List

### 1. [x] Fix `advance_tick` server-in-client-loop — **CORRECTNESS**  (`717259ea`)

**File**: `benches/src/lib.rs:400-419`

**Problem**: Server I/O (`receive_all_packets` / `process_all_packets` /
`send_all_packets`) was called K times per tick for K clients. Server
`send_all_packets` internally calls `update_entity_scopes` and iterates
*all* user_connections — so multi-user benchmarks were running this K
times per tick, inflating results.

**Fix**: All clients do their I/O in a loop, then the server runs its
tick ONCE. Landed via edit on 2026-04-24 (pending compile verification
before marking complete).

**Verify**:
- `cargo check -p naia-benches --benches`
- Run a representative bench (`cargo bench -p naia-benches --bench main -- authority/contention`) and sanity-check K=1 ≈ old K=1; K=8 should drop significantly.

---

### 2. [x] Add per-tick outgoing byte counter to Naia `Server`  (`100721d5`)

**Files**:
- `server/src/server/server.rs` (add `outgoing_bytes_last_tick()` accessor)
- `server/src/server/world_server.rs` (maintain counter inside `send_all_packets`)
- `benches/src/lib.rs` (swap `server_outgoing_bytes_per_tick()` to use new API)
- `benches/benches/wire/bandwidth.rs` + `wire/framing.rs` (no change needed — they read via BenchWorld)

**Problem**: `BenchWorld::server_outgoing_bytes_per_tick()` currently derives
a bytes-per-tick number from `outgoing_bandwidth_total()` (kbps, rolling
1-second window), which is (a) noisy for steady-state and (b) meaningless
for bursts.

**Fix**: Add a `u64` counter on the world_server that sums bytes-sent per
invocation of `send_all_packets`, and reset it at the *start* of each
`send_all_packets` call. Expose via `Server::outgoing_bytes_last_tick()`.
Caller reads this *after* a tick has run.

---

### 3. [x] Extract event-drain helper in bench harness  (commit pending)

**File**: `benches/src/lib.rs`

**Problem**: Event-drain boilerplate (`take_world_events` + `take_auths` +
`take_tick_events` for server + per-client equivalents) is duplicated
across three sites: setup connect loop (~176-208), replication-wait loop
(~241-258), `BenchWorld::tick()` (~283-289).

**Fix**: Extract `fn drain_all_events(server, clients)` into `benches/src/lib.rs`.
Call from all three sites. Saves ~30 lines; removes a class of
"forgot to drain" bugs.

---

### 4. [ ] Document magic numbers in bench harness

**File**: `benches/src/lib.rs:25-27`

**Problem**: `TICK_MS: 16`, `SETUP_TIMEOUT: 500`, `REPLICATE_TIMEOUT: 10_000`
have no rationale comments. `TICK_MS` is especially load-bearing — it
appears in the `bytes/tick` formula — so a future reader changing it
without understanding could corrupt bench results silently.

**Fix**: Add a one-line `// why` comment on each constant. Keep terse
(one line each, ~20 words max).

---

### 5. [ ] Add `--assert-wins` mode to `naia-bench-report`

**Files**: `test/bench_report/src/main.rs`, plus probably a new
`test/bench_report/src/assert_wins.rs`.

**Problem**: Wins 1–5 are *observable* in the bench output but not
*asserted*. A regression to Win-2 (O(1) idle tick) or Win-5
(immutable-component dispatch) would not fail any automated check.

**Fix**: Add `--assert-wins` flag to `naia-bench-report`. When set,
parse criterion JSON and check:

| Win | Assertion |
|-----|-----------|
| 1 (scope entry budget) | `tick/scope::enter@10000` < threshold |
| 2 (O(1) idle tick) | `tick/idle@10000` / `tick/idle@10` ratio ≤ ~3× (not linear) |
| 3 (dirty-receiver push) | `update/mutation@K users@N entities` roughly flat in N when K held fixed |
| 4 (SpawnWithComponents) | `spawn/coalesced` median < `spawn/burst` median |
| 5 (immutable dispatch) | `update/immutable` median < `update/mutation` median |

Exit non-zero on violation; print which assertion failed. Do not hard-code
thresholds for Win-1 without first observing current numbers on a clean run.

---

### 6. [ ] Expand iai coverage to Wins 3, 4, 5

**Files**: new benches in `iai/benches/`.

**Problem**: iai suite currently has `tick_hot_path.rs` and
`update_dispatch.rs` only. Wins 3 (dirty-receiver push model), 4
(`SpawnWithComponents` coalescing), 5 (immutable-component dispatch)
have no instruction-count coverage — so sub-20% regressions in those
hot paths won't be caught.

**Fix**: Add:
- `iai/benches/dirty_receiver.rs` — candidate-set scan with scoped N=10/100/1000, unscoped baseline
- `iai/benches/spawn_coalesced.rs` — one spawn-with-3-components vs spawn + 3 inserts
- `iai/benches/immutable_dispatch.rs` — single component update, mutable vs immutable kind

Each bench should use `iai-callgrind`'s `#[library_benchmark]` attribute,
match the structure of the existing two files, and keep the work-per-bench
small enough that instruction counts are stable (<1% run-to-run).

---

## Completion Protocol

When a task ships:
1. Mark `[x]` in this file with a one-line commit ref note.
2. Commit the task change together with its implementation.
3. On session resume, re-scan this file to find the next `[ ]` task.

Do NOT delete completed tasks — they are the durable record.
