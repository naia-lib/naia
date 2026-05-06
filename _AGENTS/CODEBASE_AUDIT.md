# Naia Codebase Audit — 2026-05-05

**Branch:** `release-0.25.0-e` (HEAD: `05a51856`)
**Scope:** Full naia codebase (~102K LOC across 41 crates), with emphasis on tech debt, architectural smells, and refactor opportunities. This is broader than the Replicated Resources audit (`RESOURCES_AUDIT.md`) — it surveys the whole stack.

**Author:** Claude (twin), self-directed audit at Connor's request.

**Methodology:** systematic grep + LOC analysis + manual inspection of largest files, hottest paths, and cross-cutting concerns. Findings are file:line-pointed and severity-ranked. Effort estimates are best-guess engineering hours.

---

## Severity legend

| Mark | Meaning |
|---|---|
| 🚨 | Latent bug or correctness gap. Production-reachable panic, broken contract, or shipped wrong behavior. |
| ⚠️ | High-impact architectural debt. Compounds over time; refactors here unlock other improvements. |
| 🧹 | Hygiene / dead code / pre-existing warnings. Internal-only friction. |
| 💡 | Refactor opportunity. Not broken; would meaningfully improve readability or extensibility. |
| 📚 | Documentation / discoverability gap. |
| ⚙️ | Build / CI / supply-chain concern. |
| 🔬 | Test infrastructure gap. |

---

## Tier 0 — Drop-everything fixes

### T0.1 🚨 `entity_auth_status.rs` has 15 `todo!()` panics in production paths

`shared/src/world/delegation/entity_auth_status.rs:62-92`:

```rust
pub fn can_request(&self) -> bool {
    match (self.host_type, self.auth_status) {
        (HostType::Client, EntityAuthStatus::Available) => true,
        // ... (Client, *) cases ...
        (HostType::Server, EntityAuthStatus::Available) => todo!(),
        (HostType::Server, EntityAuthStatus::Requested) => todo!(),
        (HostType::Server, EntityAuthStatus::Granted) => todo!(),
        (HostType::Server, EntityAuthStatus::Releasing) => todo!(),
        (HostType::Server, EntityAuthStatus::Denied) => todo!(),
    }
}
```

**Same shape repeated for `can_release()` and `can_mutate()`** — 15 `todo!()` arms total, all on `(HostType::Server, *)`.

If any code path with `host_type = Server` ever calls `can_request` / `can_release` / `can_mutate`, the server panics. The fact that this hasn't surfaced in tests means either (a) the code paths are unreachable in practice (in which case they should be `unreachable!()` with documented invariants), or (b) they're reachable and we have a latent crash bomb.

**Fix:** audit each arm. For genuinely unreachable arms: `unreachable!("Server-side {auth_status:?} should never reach can_request — invariant violated")` with the invariant documented inline. For reachable arms: implement correctly.

**Effort:** 2-3 hours including invariant verification.

---

### T0.2 🚨 2206 panic-paths is a massive trust-the-contract surface

| Pattern | Count |
|---|---|
| `panic!(...)` | 441 |
| `.unwrap()` | 833 |
| `.expect(...)` | 932 |
| **Total panicking call sites** | **2206** |

These are not all bugs — many are correct invariants. But this is an **enormous review surface** for a library that's meant to run server-side in production. A single off-by-one or unexpected protocol state crashes the server.

**Recommendation:** systematic audit pass. For each panic-site, verify:
1. Is the invariant documented? (Most are not.)
2. Is it actually unreachable, or is it "shouldn't happen but might"?
3. If "might": convert to `Result<...>` propagation OR `unreachable!` with explicit invariant doc.

This is a months-long effort to do completely. **Highest-leverage subset:** everything in `server/src/server/world_server.rs` (171 panic-sites in the most production-critical file) and `client/src/client.rs` (96 panic-sites).

**Effort:** triage pass ~1 day; full conversion ~1-2 weeks; ongoing convention enforcement via clippy lint or pre-push hook.

---

## Tier 1 — High-impact architectural debt

### T1.1 ⚠️ `WorldServer` god-object (3592 lines, 141 methods)

`server/src/server/world_server.rs` is the largest file in the codebase by far and the most central. Methods cover: connection handshake, scope management, entity spawn/despawn/migrate, room management, authority delegation, message send/receive, request/response, priority accumulators, resource registry, scope resolution, room user management, plus an `EntityAndGlobalEntityConverter` impl.

**Concrete decomposition** (each gets its own file):
- `world_server/connections.rs` — handshake + connect/disconnect/auth events (~300 LOC)
- `world_server/scope.rs` — `apply_scope_for_user`, `drain_scope_change_queue`, `user_scope_*`, `scope_checks_pending` (~500 LOC)
- `world_server/rooms.rs` — `make_room`, `room_mut`, `entity_room_map` interactions (~200 LOC)
- `world_server/entities.rs` — `spawn_entity`, `despawn_entity`, `enable_replication`, the entity-state lifecycle (~400 LOC)
- `world_server/authority.rs` — `entity_*_authority`, `entity_release_authority`, `transfer_auth_*`, `pending_auth_grants` (~500 LOC)
- `world_server/resources.rs` — the new resource methods (~150 LOC, already cohesive)
- `world_server/messages.rs` — `send_message`, `broadcast_message`, `send_request`, `receive_response` (~300 LOC)
- `world_server/priority.rs` — `global_entity_priority*`, `user_entity_priority*` (~150 LOC)
- `world_server/io.rs` — `receive_all_packets`, `send_all_packets`, the bytes-per-tick counters (~200 LOC)
- `world_server/mod.rs` — the `WorldServer` struct + `new()` + impl-block dispatch (~200 LOC)

Each file is now a manageable single-responsibility module. The struct stays shared across them via `impl WorldServer<E>` blocks per file.

**Effort:** 1-2 days. Bevy-style. Mechanical move + add `mod` declarations.

---

### T1.2 ⚠️ `Replicate` trait monolith (29 methods, 1499-line derive macro)

`shared/src/world/component/replicate.rs:53-128` defines a 29-method trait. The derive macro at `shared/derive/src/replicate.rs` is 1499 lines generating impls for all 29.

Trait surface is intimidating to readers and to anyone implementing manually. Splittable into:

```rust
pub trait ReplicateCore: Send + Sync + 'static + Named + Any {
    fn kind(&self) -> ComponentKind;
    fn to_any(&self) -> &dyn Any;
    fn to_any_mut(&mut self) -> &mut dyn Any;
    fn to_boxed_any(self: Box<Self>) -> Box<dyn Any>;
    fn copy_to_box(&self) -> Box<dyn Replicate>;
    fn create_builder() -> Box<dyn ReplicateBuilder> where Self: Sized;
    fn dyn_ref(&self) -> ReplicaDynRef<'_>;
    fn dyn_mut(&mut self) -> ReplicaDynMut<'_>;
    fn diff_mask_size(&self) -> u8;
    fn is_immutable(&self) -> bool { false }
}
pub trait ReplicateWrite { fn write(...); fn write_update(...); }
pub trait ReplicateRead { fn read_apply_update(...); fn read_apply_field_update(...); }
pub trait ReplicateMirror { fn mirror(...); fn mirror_single_field(...); }
pub trait ReplicateAuthority {
    fn set_mutator(...); fn publish(...); fn unpublish(...);
    fn enable_delegation(...); fn disable_delegation(...); fn localize(...);
}
pub trait ReplicateEntityRelations {
    fn relations_waiting(...); fn relations_complete(...);
}

pub trait Replicate: ReplicateCore + ReplicateWrite + ReplicateRead
    + ReplicateMirror + ReplicateAuthority + ReplicateEntityRelations {}
impl<T: ReplicateCore + ... + ReplicateEntityRelations> Replicate for T {}
```

User-facing surface unchanged — code still says `R: Replicate`. Internal sites that only need a subset can demand only what they use, making each function's contract clearer.

**Effort:** ~2 days. Touch the derive macro + the trait file + verify all internal call sites still compile.

---

### T1.3 ✅ `ComponentKinds` 64-kind hard limit — RESOLVED 2026-05-05

The hard limit is gone — fully unbounded (capped only by the `u16` NetId at 65,535, which is well past any plausible protocol).

**What landed:**
- New `shared/src/world/update/atomic_bit_set.rs` — variable-width lock-free bitset (`Box<[AtomicU64]>`) with race-tolerant "was clear" semantics.
- `AtomicDiffMask` rewritten to wrap `AtomicBitSet`. The old `debug_assert!(byte_len ≤ 8)` cap is dropped; `over_64_bits_supported_no_more_8_byte_limit` test pins the new behavior up to 256 properties.
- `DirtyQueue` refactored from a single `AtomicU64` per entity to a flat-strided `Vec<AtomicU64>` of `stride = ceil(kind_count / 64)` words per entity. `kind_bit` widened from `u8` to `u16`. Stride is `1` for ≤64 kinds, so the legacy hot path is unchanged.
- `ComponentKinds::add_component` no longer panics at 64. Added `ComponentKinds::kind_count()` accessor.
- `GlobalDiffHandler` widened `kind_bits: HashMap<_, u16>` and tracks `max_kind_count`. `UserDiffHandler::new` sizes the per-user `DirtyQueue` from this on construction.
- `dirty_receiver_candidates` rewritten to consume the multi-word drain output (`Vec<u64>` per entity, `kind_bit = word_idx * 64 + bit`).

**Tests pinning the new invariant:**
- `shared/src/world/update/atomic_bit_set.rs` — 13 unit tests including `over_64_bits_supported`.
- `shared/src/world/update/atomic_diff_mask.rs` — `over_64_bits_supported_no_more_8_byte_limit` (256 properties).
- `shared/src/world/update/mut_channel.rs::dirty_queue_unlimited_kinds_tests` — stride growth, kind_bit > 64 round-trip, cancel, multi-word `was_clear` dedup.
- `benches/tests/many_kinds_no_64_cap.rs` — end-to-end registration of 70 distinct `Replicate` types, `kind_count() == 70`, 7-bit wire tag, full ser/de round-trip.

**Hot-path cost:** zero for protocols at ≤64 kinds (stride==1, single fetch_or). For >64 kinds: one extra `Relaxed` load per `was_clear` check word. Verified by `crucible run --assert` (29/0/0 bench wins preserved).

---

### T1.4 ⚠️ `client.rs` near-clone of `world_server.rs` patterns (2311 lines, 95 methods)

`client/src/client.rs` is the second-largest file. Many of its methods mirror `WorldServer` symmetrically (entity_request_authority / entity_release_authority / configure_entity_replication / spawn_entity / despawn_entity / send_message / receive_response / per-resource registry / etc.).

The shared abstraction is `HostType::Server` vs `HostType::Client` — both sides do similar things with mirror-image semantics. There's no shared "host" base abstraction; the symmetry is implicit.

**Refactor opportunity** (large but structural): extract a `Host<E>` trait or struct that captures the common surface (entity lifecycle, send/receive, scope-or-equivalent), then `WorldServer<E>` and `Client<E>` become thin wrappers that add their unique behaviors (server: rooms + multi-user scope; client: handshake + single-server connection).

**Effort:** 1 week+. Big refactor. Defer until the WorldServer decomposition (T1.1) lands and the boundaries are clearer.

---

## Tier 2 — Code organization / dead code

### T2.1 🧹 `test/harness/legacy_tests/` is dead — 16 files, ~14K LOC NOT being run

`test/harness/legacy_tests/` contains 16 integration-test files (1011-2587 lines each, totaling ~14K LOC). `cargo test -p naia-test-harness` does NOT pick them up — Rust's convention requires `tests/` (we now have one with `replicated_resources.rs`). The `legacy_tests/` directory has no `[[test]]` declarations in `Cargo.toml`.

These tests presumably WORKED at some point — they have names like `01_connection_lifecycle.rs`, `11_entity_authority.rs`, `10_entity_delegation.rs`. They're being treated as reference reading but aren't part of the test suite.

**Three options:**
1. **Migrate to `tests/`** — rename the directory, ensure they compile, fix any bit-rot. Restores 14K LOC of integration coverage.
2. **Delete entirely** — content is captured by the namako-driven specs in `test/specs/features/` + the harness `tests/` directory.
3. **Move to `_AGENTS/REFERENCE/`** with a `README.md` flagging them as historical reference, NOT live tests.

Option 1 is the highest-value but unknown risk (likely bit-rot since they haven't been compiled). Option 2 is safest but loses the intent. Option 3 is a stopgap.

**Recommendation:** option 1 with a triage day — try to compile, cull what doesn't build, fix what mostly does. If compile-fix is >2 days of work, fall back to option 2 or 3.

**Effort:** 1-3 days depending on bit-rot severity.

---

### T2.2 🧹 6 crates literally named `app` — 6 output filename collisions on every build

```
demos/bevy/client/Cargo.toml          → name = "app"
demos/macroquad/client/Cargo.toml     → name = "app"
demos/socket/client/wasm_bindgen      → name = "app"
demos/socket/client/miniquad          → name = "app"
demos/basic/client/wasm_bindgen       → name = "app"
demos/basic/client/miniquad           → name = "app"
```

Build output: `warning: output filename collision at /home/connor/Work/specops/naia/target/debug/app` (×8 across `app`, `app.dwp`, `libapp.so`, `libapp.so.dwp`, `libapp.rlib`).

Cargo's footnote: *"this may become a hard error in the future"*. Will eventually break the build.

**Fix:** rename each to a distinct name (`naia-bevy-client-demo-app`, `naia-macroquad-client-demo-app`, etc.). Trivial mechanical change.

**Effort:** 30 minutes.

---

### T2.3 🧹 Mixed snake_case vs kebab-case crate naming — 30+ crates, no consistency

Sample from the workspace:
```
naia-shared           ← kebab-case
naia_benches          ← snake_case (bench harness)
naia_bevy_npa         ← snake_case
naia-bevy-shared      ← kebab-case
app                   ← no prefix at all (×6)
dirty_receiver        ← snake_case
local_entity_wire     ← snake_case
```

Per the user-memory feedback (`feedback_snake_case_naming.md`): "snake_case naming for all non-naia repos; as of 2026-04-26 both cyberlith and slag are 100% compliant." But naia itself is mixed.

Cargo accepts both, but consistency aids tooling, search, and grep. Public crates published to crates.io must be kebab-case (cargo enforces); internal-only test/bench crates can be either. The internal-only ones are the ones currently in snake_case.

**Recommendation:** decide and enforce. If kebab-case for all (matches public publishing convention), rename ~10 internal crates. Trivial but breaks any external `cargo build -p naia_benches` invocations.

**Effort:** 1-2 hours including any references to the renamed packages.

---

### T2.4 🧹 Pre-existing warnings carried forever

`cargo build --workspace` emits 4 long-standing warnings:
- `recycle_host_entity` never used (`shared/src/world/host/host_entity_generator.rs:138`) — verified pre-existing in a prior session.
- `naia-bevy-server` 1 warning
- `naia-shared` 1 warning
- `namako_engine` 5 warnings (4 auto-fixable via `cargo fix`)

The `recycle_host_entity` method is `pub(crate)` and genuinely unused — should either be deleted or made `pub` if it's a forward-looking public API.

**Fix:** zero-warning policy — every existing warning gets either fixed or `#[allow(...)]`'d with a justification comment. CI gate via `cargo build --workspace -- -D warnings`. Currently `cargo doc --workspace --no-deps --document-private-items` produces 1 doc warning too.

**Effort:** 1-2 hours for the fix pass + CI gate setup.

---

### T2.5 🧹 157 TODO/FIXME/HACK comments

The `shared/src/world/local/local_world_manager.rs` and `shared/src/world/delegation/entity_auth_status.rs` files have a particular concentration. Many are commented-out `todo!()` calls in match arms (decision still pending), e.g.:

```rust
(HostType::Server, false) => false, // todo!("server is attempting to publish a client-owned non-public remote entity"),
```

Several explicitly say "as far as we know" or "should never happen" — these are unverified invariants, not decisions.

**Recommendation:** triage pass. Categorize each into:
- (a) Resolved decision, delete the TODO.
- (b) Open question, file as a tracked issue in `_AGENTS/` with priority.
- (c) Genuine invariant — convert to a `debug_assert!` or `unreachable!` with rationale.

**Effort:** 1 day for the triage pass.

---

### T2.6 🧹 88 `e2e_debug` cfg gates — substantial conditional code, untested in default config

`grep -c '#\[cfg(feature = "e2e_debug"' = 88`. The feature gates instrumentation counters and trace-emission code throughout the server / client / shared. None of it builds in the default `cargo test --workspace` configuration — only when the feature is explicitly enabled (which the test_harness Cargo.toml conditionally allows).

The risk: an `e2e_debug` build that hasn't been tested in months silently breaks. The feature was useful for diagnostics during the perf-upgrade phases (per the memory entries) — if it's still useful, gate it in CI. If it's not, delete it.

**Recommendation:** add a CI matrix entry that builds + tests `--features e2e_debug` weekly. Or, if the feature is no longer needed, retire it (the code paths it gates are diagnostic-only).

**Effort:** ½ day to wire the CI matrix; alternative ½ day to remove if retired.

---

## Tier 3 — Bevy adapter specifics

### T3.1 🧹 10× `unsafe impl Send`/`Sync` in the bevy adapter — manual Send/Sync impls

```
adapters/bevy/server/src/component_event_registry.rs:26: unsafe impl Send for ComponentEventRegistry {}
adapters/bevy/server/src/component_event_registry.rs:27: unsafe impl Sync for ComponentEventRegistry {}
adapters/bevy/server/src/bundle_event_registry.rs:21:    unsafe impl Send for BundleEventRegistry {}
adapters/bevy/server/src/bundle_event_registry.rs:22:    unsafe impl Sync for BundleEventRegistry {}
adapters/bevy/shared/src/world_data.rs:37:               unsafe impl Send for WorldData {}
adapters/bevy/shared/src/world_data.rs:38:               unsafe impl Sync for WorldData {}
adapters/bevy/client/src/component_event_registry.rs:23: unsafe impl<T: Send + Sync + 'static> Send for ComponentEventRegistry<T> {}
adapters/bevy/client/src/component_event_registry.rs:24: unsafe impl<T: Send + Sync + 'static> Sync for ComponentEventRegistry<T> {}
adapters/bevy/client/src/bundle_event_registry.rs:20:    unsafe impl<T: Send + Sync + 'static> Send for BundleEventRegistry<T> {}
adapters/bevy/client/src/bundle_event_registry.rs:21:    unsafe impl<T: Send + Sync + 'static> Sync for BundleEventRegistry<T> {}
```

Each is undocumented (no SAFETY comment). The pattern is identical: registry types holding `HashMap<K, Box<dyn Trait>>` where the trait isn't auto-Send/Sync.

**Fix:**
1. Document each with a SAFETY comment explaining why the type is actually thread-safe (no interior mutability shared across threads, etc.).
2. Better: replace the trait object with a `Send + Sync` bound (`Box<dyn Trait + Send + Sync>`) so auto-Send/Sync derivation works without the `unsafe`.

Option 2 is safer. The trait objects are `ComponentEventHandler` and similar — adding `: Send + Sync` to the trait definition removes the need for the unsafe impl entirely.

**Effort:** 2-3 hours.

---

### T3.2 📚 Bevy adapter `lib.rs` files have ZERO module-level docs

`adapters/bevy/server/src/lib.rs` and `adapters/bevy/client/src/lib.rs` both have 0 `//!` lines. Users opening these files in their IDE see a wall of `pub use` re-exports with no orientation.

**Fix:** add a 20-30 line module-level doc explaining: what the crate does, the user-facing API entry points (`Plugin`, `Server`/`Client`, `commands.replicate_resource`, `add_resource_events`), the `T` client-tag generic on the client side, and the relationship to `naia-server`/`naia-client`.

**Effort:** 1 hour per file.

---

### T3.3 💡 `ComponentEventHandler` registry pattern duplicated server vs client

`adapters/bevy/server/src/component_event_registry.rs` and `adapters/bevy/client/src/component_event_registry.rs` are near-mirror images:
- Same `ComponentEventRegistry<...>` shape
- Same `ComponentEventHandler` trait
- Same `register_component_handler`/`receive_events`/`handle_inserts/updates/removes`
- Differs only in: the client side adds a `<T>` generic (client tag), takes `Vec<Entity>` vs `Vec<(UserKey, Entity)>` for inserts

Both files just gained the same D13 resource-translation logic via my Resources work, with parallel `if world.contains_resource::<Messages<*ResourceEvent<...>>>()` gates.

**Refactor:** lift the registry shape into `naia_bevy_shared` as a generic `ComponentEventRegistry<TParams>` parameterized by what each side adds (UserKey vs unit). Each adapter then declares the parameter type and reuses the body.

**Effort:** ½ day. Net deletion of ~100 lines of mirrored code.

---

### T3.4 💡 `WorldOpCommand<F>` is in `naia_bevy_shared` — extend usage

The `WorldOpCommand<F>` helper I added in Item 8 is now used for all ResourceCommands and the configure_replication path. There are still per-op Command structs elsewhere worth migrating:

- `LocalDuplicateComponents` in client commands.rs — already migrated in Item 8. ✓

Actually verified — I migrated all of them. **No remaining work for T3.4.** Listed for completeness; close as ✅.

---

## Tier 4 — Operations / supply chain / docs

### T4.1 ⚙️ `cargo-deny` advisory ignores expire **2026-06-01 (26 days)**

`deny.toml`:
```toml
ignore = [
    { id = "RUSTSEC-2024-0336", reason = "rustls 0.19 infinite-loop. ... Due 2026-06-01." },
    { id = "RUSTSEC-2026-0098", reason = "rustls-webpki name-constraint (URI). ... Due 2026-06-01." },
    # ... more entries, all due 2026-06-01
]
```

Per the comment block: "All `ignore` entries below are time-boxed to 2026-06-01. The underlying fix for every entry is the DTLS-stack migration in webrtc-unreliable-client (rustls 0.19 -> 0.23, ring 0.16 -> 0.17.12+, webpki -> rustls-webpki, reqwest 0.11 -> 0.12) and a companion bump in webrtc-unreliable (server-side openssl via native-tls)."

**Today is 2026-05-05.** 26 days until cargo-deny starts failing CI. The DTLS-stack migration in `webrtc-unreliable-client` is a substantial dependency-bump project. If it isn't underway, it should start now.

**Status check needed:** is the migration in progress on the `webrtc-unreliable-client` repo? If not, this is a **CI-breaking deadline**.

**Effort:** unknown without checking the upstream PR. The migration itself is multi-day to multi-week.

---

### T4.2 ⚙️ Pre-push hook is solid — could be CI'd too

`.git/hooks/pre-push` runs `cargo check --workspace` + `wasm32-unknown-unknown` for naia-shared/client/bevy-client. Good safety net for local commits.

CI has `.github/workflows/main.yml` + `test_coverage.yml` + `dependencies.yml` + `dependabot.yml`. **Status not checked** — recommend confirming the CI matrix covers what the pre-push covers (linux + wasm32) plus what the pre-push doesn't (test_time, e2e_debug, transport_local + transport_webrtc combinations).

**Effort:** ½ day to audit the workflows + add missing matrix entries.

---

### T4.3 📚 `README.md` is sparse (100 lines) — no architecture overview

The README has crates.io badges, install instructions, three demo run examples, and a "Testing" section. **No architecture overview.** Newcomers landing on the GitHub page don't see:
- What kind of replication (entity-component vs RPC vs both)?
- Server-authoritative vs P2P?
- Tribes-2 / Quake-style / other?
- Bevy / Hecs / standalone?
- Wire protocol (UDP / WebRTC / websocket)?

The actual answers exist scattered in code comments + `_AGENTS/` docs. Pulling them into a 1-page architecture overview in the README would dramatically improve project discoverability.

**Companion gap:** the `faq/` directory contains exactly one file (`README.md`). The README links to "FAQ" but it's effectively empty.

**Effort:** ½ day for the README + a couple of FAQ entries.

---

### T4.4 📚 No user-facing `_AGENTS/RESOURCES.md` walkthrough (E1 from prior audit)

Replicated Resources is now a flagship feature with full Mode B support, but there's no one-page user-facing doc. `_AGENTS/RESOURCES_PLAN.md` is internal design; `RESOURCES_AUDIT.md` is internal QA. The `feature` file in `test/specs/features/21_replicated_resources.feature` is the closest thing to user docs.

**Recommended structure for `_AGENTS/RESOURCES.md`:**
1. What it is (one paragraph)
2. Quickstart: derive + register + use (10 lines of code)
3. The three lifecycle ops (replicate_resource / remove / configure)
4. Authority delegation walkthrough
5. Bevy `Res<R>` semantics + comparison to vanilla Bevy Resource
6. Per-field diff (the wire test result)
7. Limitations (Mode B requires `bevy::Resource` derive)

**Effort:** 2-3 hours.

---

### T4.5 🔬 Test infrastructure proliferation — 10 different test crates

```
test/bench         test/bevy_npa     test/compile_fail  test/harness
test/loom          test/npa          test/spec_tool     test/tests
+ benches          + iai
```

Each has its own purpose, but the proliferation creates discovery friction:
- Where do I put a new replication test? `test/harness/tests/`? `test/tests/src/steps/`? `adapters/bevy/server/tests/`?
- How do I run "all the tests"? (`cargo test --workspace` doesn't run namako gate, doesn't run the `legacy_tests/` non-suite, doesn't run benches.)

A `test/TESTING_GUIDE.md` referenced by the README would orient newcomers — **but it doesn't exist** (the README links to a file that's missing).

**Fix:**
1. Write `test/TESTING_GUIDE.md` explaining each test crate's purpose + when to use which.
2. Add a `cargo xtask test-all` script (or a `test-all.sh`) that runs everything: `cargo test --workspace` + `namako gate` + bench smoke.

**Effort:** ½ day.

---

## Tier 5 — Refactor opportunities (not blockers)

### T5.1 💡 `println!`/`eprintln!` in production code path

`shared/src/transport/local/hub.rs:296, 325, 353`:
```rust
println!("[HUB] deliver_all_queued: ...");
```

This is `LocalTransportHub`, used for in-process testing. Still — tests using it dump to stdout for every packet. Should be `log::debug!` or `log::trace!`.

**Effort:** 15 minutes.

---

### T5.2 💡 12-14 features per crate — feature matrix complexity

| Crate | Feature count |
|---|---|
| client | 12 |
| server | 12 |
| shared | 14 |
| adapters/bevy/server | 6 |
| adapters/bevy/client | 6 |
| adapters/bevy/shared | 3 |

Combinatorial: 12 × 12 × 14 = 2016 distinct feature subset configurations of the core triplet. CI almost certainly doesn't cover all of them. A few are mutually exclusive (`wbindgen` vs `mquad`), most are additive.

**Recommendation:** audit which features are actually used by downstream callers. Anything with zero callers can be retired. The remaining set should have a documented compatibility matrix in `_AGENTS/FEATURES.md`.

**Effort:** ½ day for the audit; deletions afterwards as needed.

---

### T5.3 💡 434 `Box<dyn ...>` instances — vtable density audit

Many are unavoidable (Replicate dispatch, transport abstraction). Some hot-path ones may benefit from monomorphization, especially in the inner sender loop. Likely candidates:
- `Box<dyn ReplicateBuilder>` in `ComponentKinds::kind_map` — looked up every component decode
- `Box<dyn Replicate>` in `incoming_components: HashMap<(...), Box<dyn Replicate>>` (`shared/src/world/remote/remote_world_manager.rs`)

Worth a profiler run. If hot-path Box-dyn dispatch shows up, consider per-kind enum dispatch via the derive macro.

**Effort:** profile-then-decide, ~1 day for the investigation.

---

### T5.4 💡 108 `HashMap::new()` calls — initial-capacity audit

`HashMap::new()` allocates lazily but rehashes as it grows. Hot-path maps (per-tick scope, dirty-bit tracking, per-connection queues) benefit from `HashMap::with_capacity(...)` based on expected sizing.

Worth a quick scan — many are init-time and don't matter; a few in tick-loop allocators do. Cyberlith capacity numbers (1262 CCU ceiling per the recent analysis doc) bound the relevant sizes.

**Effort:** ½ day for the targeted audit.

---

### T5.5 💡 `bench_instrumentation` feature usage is sparse (9 references)

`grep "bench_instrumentation" | wc -l = 9`. Used in `server/src/connection/connection.rs` for some idle-path counters. If the benches don't currently consume these, the feature is dead-weight.

**Decision:** verify the benches use it. If not, delete. If yes, document in BENCHMARKS.md.

**Effort:** 30 minutes to verify + decide.

---

### T5.6 💡 `_AGENTS/` doc proliferation without an index

```
_AGENTS/
  ARCHIVE/
  BENCHMARKS.md
  BENCH_PERF_UPGRADE.md
  BENCH_UPGRADE_LOG/
  CRUCIBLE_BENCH_PLAN_2026-04-27.md
  CYBERLITH_BUSINESS_AND_TECHNICAL_PLAN_2026-04-26.md
  OUTPUT.md
  PROFILING.md
  RESOURCES_AUDIT.md
  RESOURCES_PLAN.md
  SYSTEM.md
```

11 top-level files + 2 directories. No `INDEX.md` or `README.md` orienting which doc is for what. New session-resumes (twin Claude pickup) waste cycles paging through files.

**Fix:** `_AGENTS/INDEX.md` (or update `SYSTEM.md`) listing every doc with a one-line purpose + status (active vs archived vs reference).

**Effort:** 1 hour.

---

## Tier 6 — Per-area scorecards

### Replicated Resources (the recently-shipped feature)

| Aspect | Score | Notes |
|---|---|---|
| User API ergonomics | A | Matches Bevy `Res<R>`/`ResMut<R>` exactly |
| Internal architecture | A- | `ReplicatedResource` trait alias, `WorldOpCommand`, single-dispatcher pattern |
| Test coverage | A | 11 harness + 4 Bevy-app + 5 namako + 7 unit = 27 tests |
| Wire correctness | A | per-field diff verified at byte level (20 vs 24) |
| Documentation | C | Plan doc exists, audit doc exists, but no user-facing README |
| Mode B exclusivity | A | No fallback path, clear panic on mis-registration |

### Server core (`server/src/`)

| Aspect | Score | Notes |
|---|---|---|
| God-object problem | F | 3592 lines / 141 methods in one file |
| Panic discipline | C | 171 panic-sites in world_server.rs alone |
| API surface coherence | B | Internal pub(crate) vs pub split is reasonable |
| Test coverage | B | Direct unit + harness; legacy_tests is dead |

### Bevy adapter (`adapters/bevy/`)

| Aspect | Score | Notes |
|---|---|---|
| User ergonomics | A- | Standard Bevy conventions; `commands.replicate_resource` etc. |
| `unsafe` discipline | C | 10× undocumented unsafe impl Send/Sync |
| Module-level docs | F | 0 lines of `//!` in lib.rs files |
| Code duplication client/server | C | Mirrored event registry, mirror-system dispatch |

### Shared (`shared/src/`)

| Aspect | Score | Notes |
|---|---|---|
| Trait surface complexity | C | 29-method `Replicate`; 1499-line derive |
| Hard-coded limits | C | 64-component DirtyQueue ceiling |
| `todo!()` panics in production | F | 15 in `entity_auth_status.rs` |
| Wire format correctness | A | Static/dynamic split, is_static tagging, per-field diff all working |

### Test infrastructure

| Aspect | Score | Notes |
|---|---|---|
| Test discoverability | C | 10 different test crates, no guide |
| Coverage of new features | A | Resources is well-covered |
| `legacy_tests/` status | F | 14K LOC of orphaned integration tests |
| Namako SDD wiring | B+ | One feature file in, 5/25 scenarios bound |

### Operations

| Aspect | Score | Notes |
|---|---|---|
| CI workflows present | A- | 3 workflows + dependabot |
| Pre-push hook | A | Linux + wasm32 |
| Supply-chain (cargo-deny) | C | RUSTSEC ignores expire 2026-06-01 (26 days) |
| README quality | C | 100 lines, no architecture overview |

---

## Suggested execution order (priority × effort)

### Sprint 1 (this week — fast wins, high impact)

1. **T0.1** entity_auth_status `todo!()` audit & fix — **2-3 hours**, eliminates production panic surface
2. **T2.2** rename 6× "app" demo crates — **30 min**, kills build warnings + future hard-error
3. **T2.4** zero-warning policy + fix existing 4 — **1-2 hours**
4. **T4.1** check status of webrtc-unreliable-client DTLS migration — **30 min if already in progress; URGENT if not**
5. **T3.1** SAFETY-comment or replace 10× unsafe impl — **2-3 hours**
6. **T5.1** `println!` → `log::trace!` in transport hub — **15 min**

**Total: ~1 working day. Closes the highest-severity items.**

### Sprint 2 (next week — structural improvements)

7. **T1.1** WorldServer decomposition into 10 files — **1-2 days**
8. **T2.1** legacy_tests/ triage (compile/cull/move) — **1-3 days**
9. **T1.3** widen DirtyQueue to 128 kinds — **4-6 hours**
10. **T4.4** user-facing `_AGENTS/RESOURCES.md` — **2-3 hours**
11. **T4.5** test infrastructure guide — **½ day**
12. **T2.3** crate naming convention pass — **1-2 hours**

**Total: ~1 week. Substantial architectural cleanup.**

### Sprint 3 (later — broader refactors)

13. **T1.2** `Replicate` trait split — **~2 days**
14. **T1.4** Host abstraction extracting `WorldServer`/`Client` commonalities — **1-2 weeks**
15. **T0.2** systematic panic-site audit — **1-2 weeks**
16. **T3.3** `ComponentEventRegistry` lift to shared — **½ day**

**Total: 3-5 weeks. Big payoff but long horizon.**

---

## Bottom line

**The codebase is in working condition** — full test sweep passes, wasm32 clean, no broken contracts on any user-visible API I exercised during the Resources work. The pieces I touched are now in good shape (post-RESOURCES_AUDIT.md fixes).

**The biggest risks are latent, not active:**
1. `todo!()` panic-paths in `entity_auth_status.rs` (Tier 0).
2. `WorldServer` god-object compounding maintenance cost over time (Tier 1).
3. `webrtc-unreliable-client` DTLS-stack RUSTSEC deadline 26 days out (Tier 4).
4. `legacy_tests/` representing 14K LOC of integration coverage in limbo (Tier 2).

**The biggest hygiene wins (cheapest with high quality return):**
1. Rename "app" demo crates (30 min, kills warnings).
2. Zero-warning policy (1-2 hours).
3. Module-level docs on bevy adapter `lib.rs` (1 hour).
4. RESOURCES.md user guide (2-3 hours).
5. Production `println!` → `log::trace` (15 min).

**About 1 working day of fast wins eliminates the most visible noise; about 1 week of structural work makes the next 6 months of feature development substantially smoother.**

The *EXTREMELY high bar* you mentioned is achievable here — the foundation is sound, the gaps are concrete, and most of them are mechanical to fix.
