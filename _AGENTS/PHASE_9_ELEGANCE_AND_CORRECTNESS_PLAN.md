# Phase 9 — Elegance & Correctness Floor

**Status:** 🔄 IN PROGRESS 2026-04-25 — 9.1 ✅, 9.2 ✅, 9.3 ✅, 9.5 ✅ COMPLETE; 9.4 🔄 Stage E shipped (commit `a21e9387`); B-strict landed (lock-free DirtyQueue, AtomicU64 bits + parking_lot, bench cells, pre-push wasm32 hook); wins gate in progress. 9.5: `naia-bevy-npa` smoke gate green (9/9) commit `ab9df334`.
**Theme:** Subtraction. Five moves, each removing more than it adds: broken tests, latent off-by-ones, eager caches with panics, half-built dead code, untested adapter surfaces.

---

## Why this phase, why now

Phase 8 closed strong (29/0/0 win-assert, -14.3% wire on `halo_btb_16v16`), but the win was earned against a soft floor:

- **`cargo test --workspace` is red.** 14 tests panic on uninitialized `TestClock`. The win-assert gate is a *perf-regression* gate — it does not verify correctness. Phase 8.3 shipped a wire-format break with namako BDD as the only correctness net. That's not enough.
- **A 5-year-old off-by-one in `bits_needed_for` shipped through every prior phase.** Every Serde-derived enum with 2/4/8/16/32 variants paid 1 extra bit per encode, undetected. Sibling bugs may exist in `structure.rs`, `tuple_structure.rs`, or `UnsignedVariableInteger<N>`.
- **`MessageContainer::bit_length` is an eager cache that panics on the read variant.** Phase 8.3 forced me to plumb `&MessageKinds` through 10 call sites to satisfy that precomputation. The cache is the disease, not the cure.
- **Phase 8.1 Stage A (`EntityIndex`) is dead code.** Defined, tested in isolation, re-exported, consumed nowhere. The aspirational mutate-path target (≤ 25 ns) is **25× off** at 638 ns. The infrastructure is half-built waiting for Stage E.
- **Bevy adapters have zero scenario coverage.** Cyberlith ships on bevy. The 8.3 wire break could have silently broken bevy adapter wiring and we wouldn't know.

The throughline: **each of these is a floor we've been building on top of without raising.** Phase 9 raises the floor before the next perf push.

---

## Phase 9 sub-phases

| Sub-phase | Goal | Headline metric | Wire impact | Log |
|---|---|---|---|---|
| 9.1 — `cargo test --workspace` green | Correctness floor: every test runs, every test passes | `cargo test --workspace` → 0 failures | — | `phase-09.1.md` |
| 9.2 — `naia_serde` derive bit-budget audit | Catch sibling off-by-ones; harden wire-format derivation | proptest harness over all derives; bit-count = theoretical-min | possible wire fix | `phase-09.2.md` |
| 9.3 — Lazy `MessageContainer` bit_length | Delete the eager cache + the `&MessageKinds` plumbing | -1 trait param, -10 call sites, -1 panic path | none | `phase-09.3.md` |
| 9.4 — Phase 8.1 Stage E (bitset `DirtySet` over `EntityIndex`) | Close the dead-code gap; meet aspirational mutate-path target | `mutate_path/single_user/single_property` ≤ 200 ns (current 638 ns) | none | `phase-09.4.md` |
| 9.5 — Bevy adapter scenario coverage | Close the scenario-test gap; add `naia-bevy-npa` | namako gate runs against `naia_npa` AND `naia-bevy-npa` | none | `phase-09.5.md` |

**Ordering rationale:** 9.1 first (everything else needs `cargo test` working). 9.2 next (wire-format audits should land before any further wire changes). 9.3 is independent and pure subtraction. 9.4 has the largest perf payoff but the most risk — gated on 9.1 + a dedicated bench baseline. 9.5 is the last brick — its bevy-adapter parity check protects everything downstream.

**Verification protocol (every sub-phase):**

```bash
# 1. Correctness floor — must stay green throughout
cargo test --workspace

# 2. Wire-format / BDD floor — must stay green throughout
cargo build --release -p naia_npa
~/Work/specops/namako/target/debug/namako gate --specs-dir test/specs --adapter-cmd target/release/naia_npa --auto-cert

# 3. Perf-regression gate — 29/0/0 must persist
cargo-criterion -p naia-benches --bench naia --message-format=json 2>/dev/null | cargo run -p naia-bench-report -- --assert-wins

# 4. wasm32-unknown-unknown build — must stay green throughout (enforced by pre-push hook)
cargo check -p naia-shared --target wasm32-unknown-unknown --features wbindgen --quiet
cargo check -p naia-client --target wasm32-unknown-unknown --features wbindgen --quiet
cargo check -p naia-bevy-client --target wasm32-unknown-unknown --quiet
```

**Hard rule:** no sub-phase merges if any of (1)–(4) regresses. 9.4 has additional bench gates documented inline.

**Pre-push hook:** `.git/hooks/pre-push` runs all four checks automatically on every `git push`.

---

## 9.1 — `cargo test --workspace` green

### Failure inventory

`cargo test -p naia-shared --lib` currently fails 14 tests, all with the same root cause:

```
panicked at socket/shared/src/backends/test_time/instant.rs:40:17:
test clock not initialized! Call TestClock::init() first.
```

Failing tests:

- `world::entity_index::tests::*` (4) — `KeyGenerator32::generate()` calls `Instant::now()` which calls `TestClock::current_time_ms()`.
- `world::sync::tests::perfect_migration_tests::*` (3)
- `world::sync::tests::real_migration_tests::*` (3)
- `world::sync::tests::bulletproof_migration::migration_handles_entity_redirects`
- `world::sync::tests::integration_migration::*` (2)
- `world::sync::tests::migration::*` (2)

### Root cause analysis

`TestClock` is a thread-local static (`socket/shared/src/backends/test_time/instant.rs:8`):

```rust
thread_local! {
    static SIMULATED_CLOCK: Cell<u64> = Cell::new(u64::MAX);
}
```

`u64::MAX` is the "uninitialized" sentinel; `current_time_ms()` panics on read if it sees that value. `cargo test` runs tests in parallel across threads — each test's thread sees its own uninitialized `SIMULATED_CLOCK`. The 5 test files that *do* call `TestClock::init(0)` work because they happen to call it before any time-touching code:

- `shared/src/connection/bandwidth_accumulator.rs:132`
- `shared/src/connection/priority_accumulator_integration_tests.rs:25`
- `benches/src/lib.rs:189`
- `test/harness/src/harness/scenario.rs:217`

The 14 failing tests don't call it. Adding `TestClock::init(0)` to each is the obvious fix but doesn't scale — it's the kind of boilerplate that gets forgotten the next time someone adds a test.

### Implementation

**Step 1 — Auto-init pattern.** Make `TestClock::current_time_ms()` lazy-init to 0 on first read in test builds, instead of panicking. This eliminates the boilerplate entirely: every test gets a sane starting clock without ceremony. Tests that *want* a specific start time still call `TestClock::init(N)` explicitly.

```rust
// socket/shared/src/backends/test_time/instant.rs
pub fn current_time_ms() -> u64 {
    SIMULATED_CLOCK.with(|cell| {
        let v = cell.get();
        if v == u64::MAX {
            cell.set(0);
            0
        } else {
            v
        }
    })
}
```

**Why not "panic with helpful message"?** Because every panic path here is a craftsman tax: it documents an invariant that should be enforced by construction. Lazy-init enforces it by construction. The 5 files that explicitly init are unaffected (they overwrite the lazy-init zero). The 14 failing tests pass without code changes.

**Step 2 — Verify no test relies on panic-on-uninit.** Grep for `TestClock::init` consumers. Confirm none assert `SIMULATED_CLOCK == u64::MAX` as a precondition. If any do (unlikely), update them.

**Step 3 — Test-isolation audit.** Run `cargo test --workspace -- --test-threads=1` and compare against `--test-threads=4`. If new failures appear at higher concurrency, document the cross-test pollution and fix at the source (likely a non-thread-local static somewhere). Memory `feedback_proactive_rigor.md` applies: don't paper over with `--test-threads=1`.

**Step 4 — Wire `cargo test --workspace` into the verification protocol.** Add it as a hard gate in any future sub-phase commit script and document it in `BENCH_PERF_UPGRADE.md` alongside `--assert-wins`.

### Files touched

| File | Change |
|---|---|
| `socket/shared/src/backends/test_time/instant.rs` | `current_time_ms()` lazy-inits to 0 instead of panicking on `u64::MAX` |
| `_AGENTS/BENCH_PERF_UPGRADE.md` | Document `cargo test --workspace` as a hard gate alongside `--assert-wins` |

### Verification

- ✅ `cargo test --workspace` exits 0
- ✅ 29/0/0 wins gate
- ✅ namako BDD gate green
- ✅ `cargo test --workspace -- --test-threads=1` and `--test-threads=auto` both pass

### Risk

Very low. The change is one function, the failure mode is well-understood, and the alternative (boilerplate `TestClock::init` in 14 tests) is what the codebase already does in 5 places without complaint.

---

## 9.2 — `naia_serde` derive bit-budget audit

### Why audit now

The Phase 8.3 fix exposed a 5-year-old bug: `bits_needed_for(variant_count)` returned `ceil(log2(variant_count + 1))` instead of `ceil(log2(max_index + 1))`, costing 1 extra bit per encode for every power-of-2 variant count. That bug shipped through every prior phase undetected by tests. The derive crate has three impl modules; only one was audited.

### Audit surface

```
shared/serde/derive/src/
├── lib.rs                    — entry, dispatches to impl modules
├── impls.rs                  — re-exports
└── impls/
    ├── enumeration.rs        — ✅ audited 2026-04-25, bits_needed_for fixed
    ├── structure.rs          — ❓ unaudited; named-field Serde
    └── tuple_structure.rs    — ❓ unaudited; tuple-field Serde

shared/serde/src/
├── number.rs                 — ❓ unaudited; SerdeInteger / UnsignedVariableInteger<N>
└── ...                       — primitive impls (bool, u8, u16, etc.)
```

### Implementation

**Step 1 — Static read of derive impls.** For each of `structure.rs`, `tuple_structure.rs`, `enumeration.rs`:
- Identify every place a bit count is computed (literal, formula, function call).
- Verify the formula is `ceil(log2(N))` for the correct N (variant count vs max index, etc.).
- For struct/tuple derives: confirm `bit_length()` is just the sum of field `bit_length()` calls — no overhead, no tag, no padding. (Structs don't carry tags; only enums do.)

**Step 2 — Static read of `SerdeInteger` and `UnsignedVariableInteger<N>`.** Read `shared/serde/src/number.rs`:
- `bit_length()` for the variable-length variant — confirm it returns the actual bits emitted, not the worst-case.
- Wire-format spec: confirm the proceed-bit semantics match `ser`/`de` exactly (ser-then-de round-trip identity is the contract).
- Boundary conditions: 0, 1, max for each tier.

**Step 3 — Property-based test harness.** New crate or test module: `shared/serde/tests/derive_proptest.rs`. For each derived `Serde` shape:

```rust
proptest! {
    #[test]
    fn enum_round_trip_and_min_bits(value: MyEnum) {
        let mut writer = BitWriter::new();
        value.ser(&mut writer);
        let bits_written = writer.bit_count();

        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let decoded = MyEnum::de(&mut reader).unwrap();

        prop_assert_eq!(value, decoded);
        prop_assert_eq!(bits_written, value.bit_length());

        // Theoretical minimum for N variants: ceil(log2(N)) bits for the tag,
        // plus the variant's payload bit_length.
        let expected_tag_bits = ceil_log2(N_VARIANTS);
        let expected_total = expected_tag_bits + value.payload_bit_length();
        prop_assert_eq!(bits_written, expected_total);
    }
}
```

Cover:
- Enums at 1, 2, 3, 4, 7, 8, 9, 15, 16, 17, 32, 33 variants (every power-of-2 boundary).
- Structs with 0, 1, many fields.
- Tuple structs with 0, 1, many fields.
- `UnsignedVariableInteger<N>` for N ∈ {1, 2, 3, 4, 8} across the full u64 range.

**Step 4 — Document the contract.** Add a doc comment at the top of `shared/serde/derive/src/lib.rs` stating the bit-budget invariant: *"For every `T: Serde`, `T.bit_length()` returns exactly the number of bits `T.ser()` writes. The proptest harness in `shared/serde/tests/derive_proptest.rs` enforces this."*

**Step 5 — Fix any bugs found.** If the harness uncovers a sibling off-by-one, fix it, update the relevant phase doc, re-run the wire bandwidth bench, and document the savings in this phase's log.

### Files touched

| File | Change |
|---|---|
| `shared/serde/derive/src/impls/structure.rs` | Audit + fix if needed |
| `shared/serde/derive/src/impls/tuple_structure.rs` | Audit + fix if needed |
| `shared/serde/src/number.rs` | Audit `SerdeInteger` bit_length vs ser |
| `shared/serde/tests/derive_proptest.rs` | **NEW** — proptest harness |
| `shared/serde/derive/src/lib.rs` | Doc comment stating bit-budget invariant |

### Verification

- ✅ `cargo test -p naia-serde-derive` passes (proptest harness)
- ✅ `cargo test --workspace` passes
- ✅ 29/0/0 wins gate
- ✅ namako BDD gate green
- ✅ If wire-format change: bench wire savings on `halo_btb_16v16_quantized`

### Risk

Low. Audit + tests are pure additions until a bug is found. If a sibling bug exists, fixing it is a wire-format break — needs to be coordinated with Cyberlith's next bump (per memory: still pre-public, fine to break).

---

## 9.3 — Lazy `MessageContainer` bit_length

### Current shape

```rust
pub struct MessageContainer {
    inner: Box<dyn Message>,
    bit_length: Option<u32>,  // Some(_) on write, None on read
}

impl MessageContainer {
    pub fn from_write(message, message_kinds, converter) -> Self {
        let bit_length = message.bit_length(message_kinds, converter);  // EAGER
        Self { inner: message, bit_length: Some(bit_length) }
    }

    pub fn from_read(message) -> Self {
        Self { inner: message, bit_length: None }
    }

    pub fn bit_length(&self) -> u32 {
        self.bit_length.expect("...should never be called on...from_read")  // PANIC PATH
    }
}
```

### Smells

1. **Two construction paths with different invariants.** `from_write` carries a precomputed `Some(_)`, `from_read` carries `None`. The struct is the same type, but one supports `bit_length()` and the other panics.
2. **Eager precomputation forces `&MessageKinds` plumbing.** Every `from_write` call site must have `&MessageKinds` in scope — Phase 8.3 plumbed it through 10 sites.
3. **The cache buys nothing the senders couldn't compute themselves.** `bit_length()` is called from exactly two places: `MessageContainer::write` when the writer is a counter, and `MessageManager::send_message` for the fragmentation decision. Both have `&MessageKinds` already.

### Implementation

**Step 1 — Inventory consumers.** Confirm the only readers of `MessageContainer::bit_length()`:
- `MessageContainer::write` (line 52 in `message_container.rs`) — counter mode pass-through.
- `MessageManager::send_message` — fragmentation threshold check.
- (Possibly) `tick_buffer_sender`, `request_sender` — verify by grep.

**Step 2 — Make `bit_length` lazy.** New API:

```rust
pub struct MessageContainer {
    inner: Box<dyn Message>,
}

impl MessageContainer {
    pub fn new(message: Box<dyn Message>) -> Self {
        Self { inner: message }
    }

    pub fn bit_length(
        &self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) -> u32 {
        self.inner.bit_length(message_kinds, converter)
    }

    pub fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        if writer.is_counter() {
            // Counter mode: write_bit is a no-op increment, so the inner
            // serialization path itself counts correctly. No separate
            // bit_length() call needed.
            self.inner.write(message_kinds, writer, converter);
        } else {
            self.inner.write(message_kinds, writer, converter);
        }
    }
}
```

Note: the `write()` branch on `is_counter()` collapses — calling `inner.write()` against a `BitCounter` is mathematically equivalent to `count_bits(bit_length())` because `BitCounter::write_bit` is a no-op-write-increment. The branch only existed because the *cached* bit_length was cheaper than re-counting. Without the cache, both branches do the same thing, so we can drop the branch entirely. **Verify this with a microbench before landing** — if the inner traversal is materially slower than the cached lookup at the senders' call frequency, keep a lightweight branch but compute lazily.

**Step 3 — Caller migration.** Replace every `MessageContainer::from_write(msg, kinds, conv)` with `MessageContainer::new(msg)`. Replace every `container.bit_length()` with `container.bit_length(&message_kinds, &mut converter)`. Eliminate the now-orphan `&MessageKinds` parameter from any call site that no longer needs it (most still need it for `write()`, but a few might).

Inventory of touched files (10 from Phase 8.3 plumbing, plus the senders):
- `client/src/client.rs` (4 sites)
- `server/src/server/world_server.rs` (3 sites)
- `shared/src/messages/channels/senders/message_fragmenter.rs` (1 site)
- `shared/src/messages/channels/senders/request_sender.rs` (2 sites)
- `shared/src/messages/message_container.rs` (the trait + impl)
- `shared/src/messages/message_manager.rs` (1 site, the bit_length consumer)

**Step 4 — Microbenchmark.** Before landing, add a temporary bench `wire/message_container/bit_length` that compares the cached vs lazy paths at realistic per-tick call counts (~180 messages per halo_btb_16v16 tick). Acceptance: lazy path within 5% of cached. If outside 5%, restore a lightweight cache **but** keep the API change (no `from_read` panic path; cache is internal).

**Step 5 — Re-run wire bandwidth bench.** Confirm zero wire-format change. The on-the-wire bytes should be byte-identical.

### Files touched

| File | Change |
|---|---|
| `shared/src/messages/message_container.rs` | Drop `bit_length: Option<u32>`; lazy `bit_length(&kinds, &mut conv)`; collapse counter branch |
| 10 call sites (per Phase 8.3 list) | `from_write(msg, kinds, conv)` → `new(msg)`; `bit_length()` → `bit_length(&kinds, &mut conv)` |
| `benches/benches/wire/message_container.rs` | **NEW** (temporary) — cached vs lazy microbench, deleted after acceptance |

### Verification

- ✅ `cargo test --workspace` passes
- ✅ 29/0/0 wins gate
- ✅ namako BDD gate green
- ✅ wire bandwidth byte-identical
- ✅ Lazy bit_length path within 5% of cached on the temp microbench

### Risk

Low-medium. The wire format is unaffected. The risk is a CPU regression on the per-message hot path. The microbench gate (Step 4) catches that before merge.

---

## 9.4 — Phase 8.1 Stage E (bitset `DirtySet` over `EntityIndex`)

### The aspirational gap

Phase 8 plan (`BENCH_PERF_UPGRADE_PHASE_8_PLAN.md` §3 targets):

| Cell | Target | Current | Multiple |
|---|---:|---:|---:|
| `mutate_path/single_user/single_property` | ≤ 25 ns | 638 ns | **25.5×** |
| `mutate_path/16_users_in_scope/single_property` | ≤ 250 ns | 4 650 ns | **18.6×** |
| `mutate_path/drain_dirty/16u_1000_dirty_entities` | ≤ 200 µs | 105 ms | **525×** |

Phase 8.1 B+C+D landed -30/-37/-9% wins but did not approach the aspirational targets. Per `project_naia_perf_phase_8.md`: *"Targets stay aspirational pending Stage E work."*

### Current dirty-tracking shape

`shared/src/world/update/mut_channel.rs:26-75`:

```rust
pub struct DirtyQueue {
    in_dirty: HashSet<(GlobalEntity, ComponentKind)>,
}
```

Every `mutate_entity` call hashes `(GlobalEntity, ComponentKind)` and inserts into a `Mutex<HashSet<...>>`. The 638 ns floor is dominated by hash computation + lock acquisition + HashSet bookkeeping. No dense indexing means no SIMD or bitset compaction is possible.

### Stage A — what's already there

`shared/src/world/entity_index.rs` defines:

```rust
pub struct EntityIndex(pub u32);  // per-user dense index
pub struct KeyGenerator32<K> { /* recycling allocator */ }
```

Tested in isolation, re-exported, **consumed nowhere**. This is the dead infrastructure.

### Stage E design

Replace per-user `DirtyQueue` with a per-user `DirtySet` keyed by `EntityIndex`:

```rust
pub struct DirtySet {
    // dirty_bits[entity_index][component_kind] — tightly packed bitset
    // Outer Vec indexed by EntityIndex (dense), inner u64 holds one bit per
    // component_kind (max 64 component kinds per protocol — assert at registry build).
    dirty_bits: Vec<u64>,

    // Sparse list of dirty entity indices for O(dirty_count) drain instead of
    // O(total_entities) bitset scan. Maintained as a Vec, not HashSet —
    // duplicates are tolerated; drain dedupes via the bitset.
    dirty_indices: Vec<EntityIndex>,
}
```

Key operations:

```rust
fn mark_dirty(&mut self, idx: EntityIndex, kind: ComponentKind) {
    let kind_bit = 1u64 << component_kind_to_index(kind);
    let slot = &mut self.dirty_bits[idx.0 as usize];
    if *slot & kind_bit == 0 {
        *slot |= kind_bit;
        if *slot == kind_bit {
            // First bit set on this entity — push to drain list.
            self.dirty_indices.push(idx);
        }
    }
}

fn drain<F>(&mut self, mut f: F) where F: FnMut(EntityIndex, u64) {
    for &idx in &self.dirty_indices {
        let bits = self.dirty_bits[idx.0 as usize];
        if bits != 0 {
            self.dirty_bits[idx.0 as usize] = 0;
            f(idx, bits);
        }
    }
    self.dirty_indices.clear();
}
```

Wins:

- **`mark_dirty`**: Vec index + 1 u64 bitwise OR + 1 conditional push. **No hash, no lock contention** if per-user (the lock is per-user, not global).
- **`drain`**: O(dirty_entities) instead of O(unique_(entity,kind)_pairs). Cache-friendly linear scan.
- **No allocation on hot path**: both vecs grow to working-set size and stay there.

Estimated CPU: `mark_dirty` should hit ~10-20 ns on warm cache, putting `mutate_path/single_user/single_property` in striking distance of the ≤ 25 ns target.

### Implementation

**Step 1 — Capture pre-9.4 baseline.**

```bash
cargo bench -p naia-benches --bench naia -- --save-baseline phase_94_pre 'update/mutate_path'
cargo criterion --message-format=json --bench naia -p naia-benches > /tmp/crit_94_pre.json
```

**Step 2 — Wire `EntityIndex` issuance into `register_component`.** Per-user, per-entity dense allocation:

```rust
// shared/src/world/sync/user_diff_handler.rs (new field):
entity_to_index: HashMap<GlobalEntity, EntityIndex>,
index_to_entity: Vec<Option<GlobalEntity>>,  // sparse, indexed by EntityIndex
key_gen: KeyGenerator32<EntityIndex>,
```

On `register_component(entity, kind)`:
- If `entity` not in `entity_to_index`, allocate new `EntityIndex` from `key_gen`, populate maps.
- Refcount per entity = number of registered component kinds; on `deregister_component` decrement and recycle index when zero.

This is **Phase 8.1 Stage A revived**, but this time with a consumer (Stage E below).

**Step 3 — Replace `DirtyQueue` with `DirtySet` keyed by `EntityIndex`.** `mut_channel.rs` rewrite. Per-user channel, so no global lock.

**Step 4 — Update mutation hot path (`world_writer.rs`, `mut_channel.rs`).** Every `mark_dirty` call site translates `GlobalEntity` → `EntityIndex` via the per-user map (already O(1) HashMap lookup; future optimization could hoist this into the caller).

**Step 5 — Update drain consumers (`UpdateChannel::process_dirty`, etc.).** New signature: `drain(|EntityIndex, u64_bitmask| { ... })`. Translate `EntityIndex` back to `GlobalEntity` via `index_to_entity` for downstream consumers that still want `GlobalEntity`.

**Step 6 — Bench gate.** Run mutate_path cells, compare against `phase_94_pre`:

| Cell | Pre target | Goal |
|---|---:|---:|
| `mutate_path/single_user/single_property` | 638 ns | ≤ 200 ns (3.2× headroom for further Stage F if any) |
| `mutate_path/16_users_in_scope/single_property` | 4.65 µs | ≤ 1.5 µs |
| `mutate_path/drain_dirty/16u_1000_dirty_entities` | 105 ms | ≤ 30 ms |

**Strict acceptance:** if any of the three cells is *worse* than pre, do not merge — diagnose and iterate. The aspirational ≤ 25 ns / ≤ 250 ns / ≤ 200 µs may not be reachable in this stage; the realistic-but-still-headline numbers above are the gate.

### 9.4 B-strict — bench redesign + lock-free notify_dirty (post Stage E checkpoint)

After Stage E landed (commit `a21e9387`, gates green, ~10–30% macro wins) the plan's ≤ 200 ns target was *not* met. Investigation showed two compounding issues:

1. **Bench harness floor (~350 ns).** The existing `update/mutate_path/single_user/single_property` cell does `world.entity_mut(...).component::<BenchComponent>()` *inside* the timed loop — a HashMap probe + downcast + criterion `iter_batched_ref` overhead that cannot drop below ~350 ns regardless of how fast `notify_dirty` becomes. The ~85 ns mutation hot path is invisible behind the harness floor.
2. **Mutex contention on `notify_dirty`.** The Stage E `DirtySet` is a `Mutex<DirtyQueue>` — every clean→dirty transition takes the per-user lock. Lock-free `Vec<AtomicU64>` for the bits (with `Mutex<Vec<EntityIndex>>` only on cold-path push) saves ~20–25 ns per transition and eliminates contention under multi-threaded mutation.

**B-strict pursues both.** Bench redesign alone makes the hot path measurable; lock-free notify alone is invisible behind the harness floor. Both together close the gap to the ≤ 200 ns target.

**Step B1 — Bench redesign.**
- Add `BenchWorldBuilder::cached_mutator(entity)` returning a held `MutSender` / component handle.
- Add cells `mutate_path/single_user/single_property_cached` and `mutate_path/16_users_in_scope/single_property_cached` using `iter_custom` with explicit batch sizes to amortize criterion overhead.
- Capture `phase_94_b_pre` baseline before any lock-free work.

**Step B2 — Lock-free `notify_dirty`.**
- Rewrite `DirtySet`:
  ```rust
  pub struct DirtySet {
      dirty_bits: Vec<AtomicU64>,             // hot path: fetch_or, was_zero from prev
      dirty_indices: Mutex<Vec<EntityIndex>>, // cold: push only on first-bit-set per entity
      capacity: AtomicUsize,                  // grown via cold-path mutex when EntityIndex exceeds vec length
  }
  ```
- `notify_dirty(entity_idx, kind_bit)`:
  - `let mask = 1u64 << kind_bit;`
  - `let prev = dirty_bits[idx].fetch_or(mask, Ordering::AcqRel);`
  - `if prev == 0 { lock dirty_indices, push idx, drop }`  // first-set-on-entity → cold path
- `drain(...)`:
  - Lock `dirty_indices`, take ownership via `mem::replace(..., Vec::new())`.
  - For each `idx`, `swap(0, Ordering::AcqRel)` on the corresponding `AtomicU64`, decode bits via `kinds_by_bit`.
- `cancel(entity_idx, kind_bit)` for deregister: atomic `fetch_and(!mask, Ordering::AcqRel)`.
- Vec growth: `allocate_entity_index` extends `dirty_bits` under a build-time lock (or `RwLock`). Hot path never resizes.

**Step B3 — Wire-format check.** This remains a CPU-only refactor. Bandwidth cells must stay byte-identical.

**Step B4 — Bench gate.** Both *cached* and *uncached* cells must improve vs `phase_94_b_pre`. Targets:

| Cell | Pre (post Stage E) | B-strict goal |
|---|---:|---:|
| `mutate_path/single_user/single_property_cached` (NEW) | TBD (≈ 85 ns expected) | ≤ 60 ns |
| `mutate_path/single_user/single_property` (legacy) | ~683 ns | ≤ 425 ns (harness floor) |
| `mutate_path/16_users_in_scope/single_property_cached` (NEW) | TBD | ≤ 0.7 µs |
| `mutate_path/16_users_in_scope/single_property` (legacy) | 3.67 µs | ≤ 2.0 µs |
| `mutate_path/drain_dirty/16u_1000_dirty_entities` | 98.49 ms | ≤ 70 ms |

**Step B5 — 29/0/0 wins gate** (must persist).

**Step B6 — Doc + commit.** Append B-strict section to `phase-09.4.md` with redesigned-bench numbers + harness-floor explanation. Commit + push.

**Step 7 — 29/0/0 wins gate.** No regressions in any other cell.

**Step 8 — Wire byte-identical check.** This is a CPU-only refactor. Capture wire bytes from `bandwidth_realistic_quantized/halo_btb_16v16` pre and post; assert byte-identical.

**Step 9 — Component-kind cap.** The bitset design assumes ≤ 64 component kinds per protocol (one u64 per entity). Add an assertion at `ComponentKinds::add_component`:

```rust
assert!(self.current_net_id < 64, "Stage E DirtySet supports max 64 component kinds; protocol has {}", self.current_net_id);
```

Cyberlith ships ~6-8 component kinds; this is comfortable. If a future protocol exceeds 64, the assertion documents the cliff, and we extend to two u64s or a small Vec<u64>.

### Files touched

| File | Change |
|---|---|
| `shared/src/world/sync/user_diff_handler.rs` | Plumb `EntityIndex` issuance + maps |
| `shared/src/world/update/mut_channel.rs` | Replace `DirtyQueue` with `DirtySet` |
| `shared/src/world/update/update_channel.rs` (or equivalent drain consumer) | New `drain` API consumes `(EntityIndex, u64)` |
| `shared/src/world/component/component_kinds.rs` | Assert ≤ 64 kinds |
| `_AGENTS/BENCH_UPGRADE_LOG/phase-09.4.md` | Log + measurements |

### Verification

- ✅ `cargo test --workspace` passes
- ✅ 29/0/0 wins gate
- ✅ namako BDD gate green
- ✅ Wire bytes identical to pre-9.4
- ✅ All three `mutate_path` cells improved vs pre-9.4 (gate above)
- ✅ Realistic targets met or documented why not

### Risk

**Highest risk in Phase 9.** This rewrites the dirty-tracking core. Mitigations:

1. **Lock the wire format with a byte-identical check.** This is a CPU-only refactor.
2. **Capture a dedicated baseline** (`phase_94_pre`) before starting and bench against it after every commit during the rewrite.
3. **Gate strictly on no-regression in any of 29 wins.** If an unrelated cell regresses, that's a sign the abstraction leaked somewhere.
4. **Component-kind assertion.** The 64-kind cap is documented at registry build, not buried in dirty-set code.

---

## 9.5 — Bevy adapter scenario coverage

### Coverage gap

`namako gate` runs against `naia_npa` (test/npa/src/), which uses the `naia-test-harness` directly. Cyberlith ships on `naia-bevy-{client,server}`. Today there is **no scenario-level coverage** of the bevy adapter wiring — every Phase has been validated against the harness path, never against the bevy plugin path.

The 8.3 wire-format break could have silently broken bevy adapter wiring (e.g., if the bevy plugin internally bypassed `Protocol::message_kinds` for any reason). Cyberlith would catch it on next bump; Naia CI wouldn't.

### Implementation

**Step 1 — Build `naia-bevy-npa`.** New crate at `naia/test/bevy_npa/`. Mirrors `naia_npa`'s manifest/run contract but routes through `naia-bevy-server` + `naia-bevy-client` plugins instead of the harness directly.

Shape:

```rust
// test/bevy_npa/src/main.rs — same CLI as naia_npa
//   bevy_npa manifest
//   bevy_npa run --plan plan.json --output report.json

// test/bevy_npa/src/world.rs — wraps a minimal bevy App with:
//   - SharedPlugin + ServerPlugin (or ClientPlugin)
//   - Same protocol as naia_npa (shared via naia-tests)
//   - System(s) that drain bevy events (SpawnEntityEvent, MessageEvents, ...)
//     and translate them into the same step-dispatch results as naia_npa
```

The key insight: `naia-tests::TestWorld` is the source of truth for step semantics. `naia-bevy-npa` doesn't duplicate that — it adapts bevy's event stream into the same `TestWorld` interface, so the same step library runs.

**Step 2 — Run namako gate against both adapters.** New CI step:

```bash
namako gate --specs-dir test/specs --adapter-cmd target/release/naia_npa --auto-cert
namako gate --specs-dir test/specs --adapter-cmd target/release/naia-bevy-npa --auto-cert
```

Both must pass.

**Step 3 — Coverage parity check.** Audit which steps `naia_npa` supports that `naia-bevy-npa` does not. Document gaps. If a step is unimplementable through the bevy plugin (genuinely doesn't exist in that surface), document and skip; otherwise implement.

**Step 4 — Bench the bevy adapter end-to-end.** New bench `bevy_adapter/full_tick/halo_btb_16v16` that runs a halo-shaped scenario through the bevy plugin and measures full tick cost. This catches future regressions where a bevy-system reorder or plugin change adds per-tick overhead. Initially, this is informational — no win-assert gate.

### Files touched

| File | Change |
|---|---|
| `test/bevy_npa/Cargo.toml` | **NEW** — crate manifest |
| `test/bevy_npa/src/main.rs` | **NEW** — CLI entry mirroring `naia_npa` |
| `test/bevy_npa/src/world.rs` | **NEW** — bevy App adapter |
| `Cargo.toml` (workspace) | Add `test/bevy_npa` to members |
| `benches/benches/bevy_adapter/full_tick.rs` | **NEW** — informational bench |
| `_AGENTS/BENCH_UPGRADE_LOG/phase-09.5.md` | Log |

### Verification

- ✅ `cargo test --workspace` passes
- ✅ 29/0/0 wins gate
- ✅ namako BDD gate green for **both** `naia_npa` and `naia-bevy-npa`
- ✅ Coverage-parity table documented (which steps work where)

### Risk

Low. Pure additive coverage. The risk is the bevy adapter has bugs we discover the moment we test it for the first time — but discovering them now is exactly the point.

---

## Cross-cutting verification protocol

Every sub-phase commit must pass:

```bash
# 1. Correctness floor
cargo test --workspace

# 2. BDD floor (both adapters after 9.5)
~/Work/specops/namako/target/debug/namako gate \
  --specs-dir test/specs \
  --adapter-cmd target/release/naia_npa \
  --auto-cert
# (after 9.5)
~/Work/specops/namako/target/debug/namako gate \
  --specs-dir test/specs \
  --adapter-cmd target/release/naia-bevy-npa \
  --auto-cert

# 3. Perf-regression gate
cargo criterion --message-format=json --bench naia -p naia-benches \
  2>/tmp/crit-stderr.log > /tmp/crit-stdout.json
cargo run --release -p naia-bench-report -- --assert-wins < /tmp/crit-stdout.json

# 4. Wire-format byte-identical check (sub-phases 9.3, 9.4)
cargo bench -p naia-benches --bench naia -- 'wire/bandwidth_realistic_quantized/scenario/halo_btb_16v16'
# Compare B/tick to pre-sub-phase value; must be byte-identical.
```

**Hard rules:**
- No sub-phase merges if any gate regresses. Diagnose, fix, re-run before commit.
- Wire-format breaks are explicit and called out in the phase log; otherwise byte-identical is the contract.
- Perf changes (improvements OR regressions) get measured deltas in the phase log — no hand-waving.

---

## Sequencing & estimated effort

| Order | Sub-phase | Estimated effort | Rationale for order |
|---|---|---|---|
| 1 | 9.1 — `cargo test` green | 0.5 day | Floor for everything else |
| 2 | 9.2 — derive audit | 1 day | Wire-format audits before any wire change |
| 3 | 9.3 — lazy `MessageContainer` | 0.5-1 day | Independent; pure subtraction |
| 4 | 9.4 — Stage E DirtySet | 3-5 days | Largest payoff, highest risk; needs 9.1 floor |
| 5 | 9.5 — bevy adapter coverage | 1-2 days | Parity check; protects everything downstream |

**Total:** ~7-10 working days.

**Parallelization:** 9.2 and 9.3 are independent and can be worked in parallel. 9.5 depends on nothing structurally and could be started as soon as 9.1 lands.

---

## Sign-off criteria for Phase 9

Phase 9 closes when:

- [ ] `cargo test --workspace` is green and stays green
- [ ] Proptest harness covers every Serde-derive shape
- [ ] `MessageContainer` no longer has an eager `bit_length` cache or a panic-on-read path
- [ ] `EntityIndex` is consumed by `DirtySet` (Stage E landed); aspirational mutate_path targets met or documented
- [ ] `naia-bevy-npa` exists and runs the same namako BDD scenarios as `naia_npa`
- [ ] All 29 wins persist; `halo_btb_16v16_quantized` byte-identical to post-8.3
- [ ] Memory `project_naia_perf_phase_8.md` superseded by `project_naia_phase_9.md`
- [ ] `_AGENTS/BENCH_UPGRADE_LOG/phase-09.{1..5}.md` each document final numbers + deltas

The throughline at sign-off: **less code than at the start of Phase 9, more correctness, more coverage, faster mutate path.** Subtraction wins.
