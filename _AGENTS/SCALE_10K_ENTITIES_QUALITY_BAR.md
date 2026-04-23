# Engineering Quality Bar — Scaling Naia to 10K+ Entities

**Status:** Proposed — 2026-04-23
**Companion to:** `SCALE_10K_ENTITIES_PLAN.md` (what to do), `SCALE_10K_ENTITIES_TEST_STRATEGY.md` (how to not break stuff).
**Purpose:** Concrete engineering hygiene, tooling, and discipline that must accompany the refactor so the Naia codebase lands in a strictly better state across architecture, tests, CI, observability, and documentation — not just functionally faster.

This is the "don't wave your hands" layer. Every item here is verifiable, gated, and specific to things Naia doesn't have today.

---

## 1. Pre-flight code-hygiene audit

Before any Phase 1 code lands, walk the files the refactor touches and resolve tech debt that would otherwise compound. These are not nice-to-haves; they're liabilities the refactor would carry forward.

### 1.1 Nine `todo!()` in `shared/src/world/host/host_world_manager.rs:268-292`

```rust
EntityMessage::Spawn(_) => {
    todo!("Implement EntityMessage::<HostEntity>::Spawn handling");
}
// ... 8 more variants: Despawn, InsertComponent, RemoveComponent,
// Publish, Unpublish, EnableDelegation, DisableDelegation, SetAuthority
```

This is the host-side `process_incoming_messages` branch. Phase 1 touches delegation and Phase 4 adds a new `EntityMessage` variant — both of which increase the likelihood of this branch being hit.

**Required before Phase 1:**
- Determine for each variant: is the branch genuinely unreachable (delete), or is it a real hole (implement)?
- If unreachable, replace `todo!()` with a documented `unreachable!()` naming the invariant.
- Add a `#[cfg(debug_assertions)]` log at the `match` entry so if a message variant *does* reach here in prod, we see it in our telemetry before the `unreachable!()` panics.

**Acceptance:** zero `todo!()` in files the refactor touches. The 11 other `todo!()` occurrences across the repo are out-of-scope but noted.

### 1.2 Commented-out / dead code sweep

`shared/src/world/host/host_world_manager.rs:374-420` — `fn on_delivered_migrate_response` has dead commented-out code paths. Either delete or implement.

`shared/src/world/update/user_diff_handler.rs:76-78` — `pub fn has_diff_mask` commented out. Delete.

`shared/src/world/update/global_diff_handler.rs:37-41` — commented-out `info!` call. Either rewire via the `log` crate properly or delete.

**Acceptance:** `grep -n "^\s*//.*pub fn\|^\s*//.*fn\|^\s*//.*info!\|^\s*//.*todo!" shared/src/` returns nothing in files touched by the refactor.

### 1.3 `process_delivered_commands` in `host_world_manager.rs:194-253`

Reviewing for the refactor: this function fan-outs delivered messages to `on_delivered_spawn_entity` (a stub), `on_delivered_despawn_entity`, `on_delivered_insert_component` (a no-op with a comment), `on_delivered_remove_component`. The stubs are load-bearing — the system works today because the stubs happen to be correct — but the shape is confusing.

**Required:** either fold the stubs inline with a clarifying comment, or keep the dispatch but remove the stubs that do nothing. This file's complexity is about to grow with the pause/resume machinery; clean it now while we have the context.

### 1.4 Rustdoc gaps on touched types

`ReplicationConfig`, `PropertyMutator`, `MutChannel`, `HostEntityChannel`, `GlobalEntityRecord`, `UserDiffHandler` — the refactor adds to these and exposes some to a wider surface. Today most have a one-line `///` or none.

**Required:** for each type that the refactor touches, the post-refactor state has at minimum:
- A paragraph `///` on the type itself explaining role + lifetime.
- `# Examples`, `# Panics`, `# Errors` sections on public methods where applicable.

---

## 2. Missing CI gates

### 2.1 `naia_spec_tool verify` must run in CI

Today's `.github/workflows/main.yml` runs fmt + clippy + tests per-package + wasm targets. It does NOT run `cargo run -p naia_spec_tool -- verify`. Policy B (every contract obligation has a labeled test) is enforced only locally by convention.

**Required:** add a job to `main.yml`:

```yaml
spec-verify:
  name: Spec Adequacy
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v3
    - uses: Swatinem/rust-cache@v2
    - name: Verify specs, tests, adequacy
      run: cargo run -p naia_spec_tool -- verify
    - name: Strict adequacy
      run: cargo run -p naia_spec_tool -- adequacy --strict
```

Land this *before* Phase 0 so gap-closure work has the gate active from the start.

### 2.2 `traces check` must run in CI

Per the test strategy, golden wire traces are the regression fence for Phases 2/3/4. CI must enforce:

```yaml
- name: Golden traces
  run: cargo run -p naia_spec_tool -- traces check
```

### 2.3 `cargo-audit` + `cargo-deny`

Every recent push triggered a GitHub banner about 2 unresolved vulnerabilities (1 high, 1 moderate) on the default branch. These are compounding risk. Add a workflow:

```yaml
supply-chain:
  name: Supply Chain
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v3
    - name: cargo-audit
      uses: rustsec/audit-check@v1
    - name: cargo-deny
      uses: EmbarkStudios/cargo-deny-action@v1
```

Add a `deny.toml` at workspace root excluding known-and-accepted advisories with a written justification inline.

### 2.4 Tighten clippy beyond current allows

Current `RUSTFLAGS: -Dwarnings -Aclippy::new_without_default -Aclippy::derive-partial-eq-without-eq`. Add:

```yaml
env:
  RUSTFLAGS: "-Dwarnings -Wclippy::pedantic -Aclippy::module_name_repetitions -Aclippy::missing_errors_doc"
```

Pedantic lints catch a lot that plain clippy doesn't. Opt out only the noisy ones; keep the signal.

### 2.5 `cargo doc --no-deps` as a CI check

```yaml
- name: Docs build clean
  run: cargo doc --workspace --no-deps
  env:
    RUSTDOCFLAGS: "-D warnings"
```

Rustdoc warnings-as-errors catches broken intra-doc links and unwritten `# Panics` sections.

---

## 3. Benchmarking discipline

Naia has **no `benches/` directory**. The perf claims in the plan ("10K tiles with O(mutations) work") are today unverifiable. This is unacceptable for a networking library and is the single biggest methodological gap.

### 3.1 Add criterion benches crate

Create `benches/` with a dedicated `naia-benches` crate using `criterion`:

```
benches/
  Cargo.toml           # separate from workspace default-members to avoid build-time cost
  src/
    common.rs          # scenario fixtures (10K entities, N users, etc.)
  benches/
    scope_update.rs    # scope-update work per tick
    update_dispatch.rs # mutation → update pipeline
    level_load.rs      # 10K-entity burst (wire bytes + reliable-message count)
    per_component.rs   # DiffHandler allocations per component insert
```

### 3.2 Baseline-per-phase

For each Phase that claims a perf improvement, the PR commits a baseline criterion report *before the code change* (generated on `main`) AND the post-change report. Include both in the PR description so the review can diff them.

```bash
cargo bench --bench scope_update -- --save-baseline pre-phase-2
# <land Phase 2 code>
cargo bench --bench scope_update -- --save-baseline post-phase-2
cargo bench --bench scope_update -- --baseline pre-phase-2
```

Concrete claims to verify per phase:
- **Phase 2:** idle-room scope-update tick time ≤ 5% of pre-refactor (and absolute < 100µs for 10K entities).
- **Phase 3:** idle-entity per-tick update-dispatch tick time ≤ 5% of pre-refactor.
- **Phase 4:** level-load wire bytes for 10K entities drop by ≥ expected framing overhead (specific number TBD during implementation).
- **Phase 5:** `DiffHandler.mut_receiver_builders.len()` after inserting 10K immutable components = 0.

### 3.3 Perf regression gate in CI (deferred, not blocking)

Criterion can be configured to fail CI on regression. Not required for the refactor, but worth adopting as a follow-up once a baseline exists. Flagged for `_AGENTS/` backlog.

---

## 4. Concurrency correctness — loom for Phase 3

`MutChannel::send` runs on whatever thread mutates a `Property<T>` — in Bevy that's the system-scheduler-chosen thread. Today the cross-thread communication is via `Arc<RwLock<DiffMask>>` (one per user per component). Phase 3 adds a per-user `dirty_components: HashSet<(GlobalEntity, ComponentKind)>` that is written from the mutation thread and drained from the send thread.

This is a new synchronization site on a hot path. Current tests don't exercise it under interleaving.

### 4.1 Add loom test crate

`loom` is the standard Rust tool for model-checking concurrent code. It exhaustively permutes thread interleavings on unit-test-scale models.

```
test/loom/
  Cargo.toml       # depends on loom, gated behind "loom" feature
  src/
    dirty_set.rs   # model-check the per-user dirty set writer/drainer
    mut_channel.rs # model-check the existing MutChannel path (retroactive confidence)
```

Run as part of a dedicated CI job (loom runs slow — minutes, not seconds):

```yaml
loom:
  runs-on: ubuntu-latest
  steps:
    - run: cd test/loom && cargo test --features loom --release
```

### 4.2 Required cases

- Writer on thread A mutates property P1 and P2; drainer on thread B drains; final state contains both.
- Writer-drainer interleaving does not lose a mutation (lost-update invariant).
- Concurrent writers from multiple threads (Bevy systems) to different entities — no torn state.

---

## 5. Observability — tracing + metrics

### 5.1 Migrate hot paths to `tracing`

Naia uses `log` today with zero uses of `tracing`. For the refactored code, `tracing` spans let us measure tick-level cost breakdown in prod without rebuilds:

```rust
#[tracing::instrument(skip_all, fields(queue_len = self.scope_change_queue.len()))]
fn drain_scope_changes(&mut self) { ... }

#[tracing::instrument(skip_all, fields(dirty_entities = dirty.len()))]
fn drain_dirty_updates(&mut self) -> ... { ... }
```

Scope:
- Top-level server tick (`WorldServer::send_all_updates`).
- `drain_scope_changes` (new, Phase 2).
- `drain_dirty_updates` (new, Phase 3).
- `write_updates` / `write_commands` per-packet.
- `init_entity_send_host_commands` (Phase 4 boundary).

Keep `log::warn/info` for error cases; `tracing` is for structured perf data. Add `tracing-subscriber` as a dev-dep only; production users configure their own subscriber.

### 5.2 Expose new metrics via observability contract (05)

Contract `05_observability_metrics.spec.md` has 11 obligations and 7 scenarios today. The refactor adds prod-useful metrics that should be contracted:

- Gauge: `scope_change_queue_depth`
- Gauge: `dirty_entities_per_user` (histogram across users)
- Gauge: `diff_handler_receiver_count_global`
- Gauge: `diff_handler_receiver_count_per_user`
- Counter: `spawn_with_components_total`

These go into the existing `ServerMetrics` / `ClientMetrics` surface. Add obligations to contract 05 as part of each Phase's deliverable.

---

## 6. Wire-format safety (Phase 4 specifically)

Phase 4 adds `EntityCommand::SpawnWithComponents` — a new variant in a reliable-channel message type. This expands the wire-decode attack surface.

### 6.1 Fuzz the new decoder

Add a `cargo-fuzz` target for `WorldReader::read_command`:

```
fuzz/
  Cargo.toml
  fuzz_targets/
    world_reader.rs
```

Run for N minutes in CI on a cadence (weekly, not per-PR). The fuzz target feeds random bytes into `read_command` and asserts no panic. Specifically covers:
- Truncated `SpawnWithComponents` payloads.
- Component-count field larger than remaining buffer.
- Unknown `ComponentKind` values.
- Mixed valid/invalid inside the same command.

### 6.2 Fragmentation scenario for SpawnWithComponents

Contract 03 (Messaging) covers fragmentation for long messages. A 10K-tile-ish SpawnWithComponents could theoretically be small or large depending on component sizes. Add a scenario to contract 18:

- **t6:** `SpawnWithComponents` with payload exceeding MTU is fragmented and reassembled with no observable difference from small-payload case.

### 6.3 Protocol version bump documentation

Naia's `ProtocolId` is a hash of the protocol definition. Adding the new variant naturally changes the hash, which naturally rejects old clients at handshake. Document this explicitly in:

- `SCALE_10K_ENTITIES_PLAN.md` §6 Open Questions — note resolved.
- Contract 18 obligation t5 (already listed).
- Release notes on the version that ships Phase 4.

---

## 7. API documentation + migration guide

### 7.1 Migration guide for downstream

Add `docs/migration-v0.26.md` (or whatever version ships this) covering:

- Struct-ified `ReplicationConfig` — migration path from enum. `const fn` constructors preserve the call-site shape.
- New `.persist_on_scope_exit()` — what it does, when to use it, interaction with `Delegated`.
- New `#[component(immutable)]` — Bevy-side enablement, Naia-side requirements, forbidden field types.
- `SpawnWithComponents` — zero user-facing API change; mentioned for release-note transparency.
- `UserScope::include/exclude` — unchanged, but idle-tick cost is now O(1) (documentation update, not API).

### 7.2 Canonical example: cyberlith tile

Add `demos/10k_entities_demo/` that spawns 10K `#[component(immutable)]` entities with `ReplicationConfig::public().persist_on_scope_exit()` across multiple users with scope changes. Doubles as a manual perf-verification scenario and a reference for downstream users.

### 7.3 `#[deny(missing_docs)]` on new modules

Each new file (`replication_config.rs` after the struct refactor, the new `scope_exit.rs` if we split it, the Phase 3 dirty-set module) gets `#![deny(missing_docs)]` at the top. Forces rustdoc on every public item. Existing code continues under `#[warn(missing_docs)]` at the crate root — new code raises the bar without forcing a retroactive doc sprint.

---

## 8. Feature-flag rollout

For Phases 2 and 3 (behavior-transparent internal changes), shipping behind a cargo feature flag lets downstream A/B test and revert without a repo-level revert.

### 8.1 Flag: `v2_push_pipeline`

```toml
# server/Cargo.toml
[features]
v2_push_pipeline = []  # default off during stabilization window
```

Phase 2 gates its code paths behind `#[cfg(feature = "v2_push_pipeline")]`. Phase 3 same. Phase 1, 4, 5 are user-visible API changes and do NOT gate (no meaningful "fall back to the old shape" story).

### 8.2 Stabilization criteria

Flag becomes default-on and the old paths get deleted when:
- All Phase 0 tests pass on both flag states.
- Golden traces match on both flag states.
- One release cycle elapsed with the flag off-by-default.
- No critical bug reports filed against the flag-on path.

This is an additional ~2 PRs (one to flip default, one to delete the old path), but the optionality is cheap and the confidence boost is real.

### 8.3 Anti-pattern to avoid

Flags proliferate. We allow exactly one (`v2_push_pipeline`). Phase 1, 4, 5 ship as hard cuts. Phase 5 does NOT get a "soft immutable" flag — the type-system mechanism is the commitment.

---

## 9. Per-phase PR checklist

Every phase PR must have all of these present at review time. Missing any = return for fixes.

```markdown
## Phase N — [Title]

### Code
- [ ] Implementation matches the Phase N scope in `SCALE_10K_ENTITIES_PLAN.md` §3
- [ ] No `todo!()`, `unimplemented!()`, or dead commented code added or left behind
- [ ] New public items have rustdoc with `# Examples` / `# Panics` / `# Errors`
- [ ] `tracing` spans added on hot paths (§5.1)

### Tests
- [ ] All existing contracts still pass (attach `naia_spec_tool verify` output)
- [ ] New contract `NN_...spec.md` added with obligations t1..tN
- [ ] New feature scenarios match obligations
- [ ] New step bindings (if any) have their hashes stable across runs
- [ ] Compile-fail fixtures (Phase 5 only) all produce expected errors

### Benchmarks
- [ ] `cargo bench` baseline captured on parent commit
- [ ] `cargo bench` baseline captured on this commit
- [ ] Delta meets Phase N acceptance claim from §3.2
- [ ] Benchmark results included in PR description

### Golden traces (Phases 2/3/4)
- [ ] `traces check` passes (Phases 2/3) or new goldens committed with diff explanation (Phase 4)

### Concurrency (Phase 3)
- [ ] `loom` tests added under `test/loom/`
- [ ] Loom CI job passes

### Supply chain
- [ ] `cargo audit` clean
- [ ] `cargo deny check` clean

### Docs
- [ ] Migration guide updated if user-visible API changed
- [ ] Demo added or updated (Phase 1 or Phase 5 only)
```

Enforce via a PR template committed to `.github/pull_request_template.md` that pulls in the relevant subset per phase via link.

---

## 10. What the codebase looks like after

If every item in this plan + this doc is honored, the Naia repo at the end of Phase 5:

**Coverage.** Contracts 06, 07, 08, 09, 10, 11 have full obligation coverage with running BDD scenarios. Five new contracts (15–19) specify the new behavior. Zero `@Deferred` in any feature file. `adequacy --strict` is CI-enforced.

**Observability.** Every hot-path tick phase has a tracing span. Metrics for dirty-set depth, scope-change-queue depth, diff-handler receiver counts are surfaced. Contract 05 covers them.

**Performance.** Criterion baselines exist for every claim in this plan. 10K-entity idle-room tick is proportional to mutations, not entities. Per-component allocation for immutable tiles is zero.

**Safety.** Loom-verified push-based dirty set. Fuzz-tested wire decoder for `SpawnWithComponents`. Nine `todo!()` gone. Nine `unreachable!()` in their place, each with an explicit invariant.

**Supply chain.** `cargo-audit` and `cargo-deny` gated. Vulnerability banner gone.

**Docs.** Every public item touched has rustdoc with examples. Migration guide ships with the release. Demo crate exercises 10K entities + immutable + persist.

**Architecture.** `ReplicationConfig` is a struct with orthogonal axes (`publicity`, `scope_exit`). Scope-change propagation is push-driven. Update dispatch is push-driven. Immutable components are type-system-enforced and allocation-free. The per-client metadata layer scales with *what changes*, not with *what exists*.

That's the bar. Hold it.

---

## 11. Appendix — what this plan deliberately does NOT add

To prevent scope creep during implementation. Items here are either unnecessary given what's already in the plan, or explicit out-of-scope.

- **Property-based testing (proptest)** beyond what the strategy doc's §7 open-question discusses. Scenario coverage + loom + fuzz is sufficient for Phase 1–5. Add proptest as a follow-up if a bug pattern emerges that scenarios miss.
- **async-mutex migration.** Naia's `RwLock` is `std::sync`. No benefit to switching for this refactor.
- **No-alloc / no-std paths.** Out of scope. Server already depends on std.
- **Typestate-encoded authority state machine.** Considered for `EntityAuthStatus`. The pay-off doesn't justify the API churn; contract 11 scenario coverage is the right tool.
- **Replacing `log` with `tracing` wholesale.** Only the new hot paths migrate. The rest stays `log`.
- **Removing `todo!()` elsewhere in the codebase.** 11 total exist; we address only the 9 in `host_world_manager.rs` that the refactor touches. The other 2 are unrelated and get their own cleanup ticket.
