# Naia — Full Codebase Audit Plan 2026

**Branch:** `dev`  
**Audit baseline:** HEAD `015108ef` (2026-05-09)  
**Purpose:** A clean-slate, systematic audit of the naia codebase after the most intensive development period in the project's history. Everything in §Baseline is done and must not be re-audited.

---

## The Five Goals

The audit is organized around five goals, in this priority order. Every finding should be classified against one of them:

| # | Goal | What "shock people" looks like here |
|---|---|---|
| **G1** | Architectural & implementation rigor | Clean module boundaries, sound invariants, zero latent bugs, consistent patterns |
| **G2** | Test coverage | Every behavior is verified, every edge case is named, no dark corridors |
| **G3** | API design | Minimal, cohesive, unsurprising, Rust-idiomatic, equally elegant in standalone and Bevy |
| **G4** | Performance | Zero-waste hot paths, optimal wire encoding, benchmark numbers that read like boasts |
| **G5** | Documentation | Professional, accurate, complete — a newcomer could build a real game from it |

---

## Baseline — what's already done (do not re-audit)

| Area | Status |
|---|---|
| Production `todo!()` | **0** — T0.1 closed in P1 |
| `unsafe impl Send/Sync` | Removed — P8 |
| 64-kind component limit | Removed — T1.3 (AtomicBitSet + DirtyQueue stride) |
| Legacy test suite | Retired — SDD migration |
| BDD gate | **309 active scenarios**, 100% pass (core); **21 scenarios** (Bevy NPA) |
| Build warnings | **0** (`-D warnings` enforced) |
| `cargo-deny` advisories | Extended to **2027-06-01** |
| Documentation | D-P10 complete: README, CONCEPTS.md, SECURITY.md, CHANGELOG.md, MIGRATION.md, all `//!` + `///` API surface |
| println! → debug! | Done — P11 |
| Naming (kebab-case) | 3 crates renamed — P11 |
| Reconnect edge cases | 3 BDD scenarios — P9 |
| Publication bug | Fixed — P4 (`unpublish_entity` / `publish_entity`) |
| Authority `give_authority` | Implemented — P1 |
| Perf upgrade | Phases 0–10 complete; 6,356× idle improvement; 29/0/0 bench wins |
| Priority accumulator | Fiedler pacing A+B+C — complete |

---

## Pre-Flight (run before any track)

```bash
# Confirm green baseline
git log --oneline HEAD~10..HEAD

cargo build --workspace --all-targets 2>&1 | grep "^warning"
# Expected: zero output

namako gate --specs-dir test/specs \
  --adapter-cmd "cargo run --manifest-path test/npa/Cargo.toml --"
# Expected: 309 active, 100% pass

namako gate --specs-dir test/bevy_specs \
  --adapter-cmd "cargo run --manifest-path test/bevy_npa/Cargo.toml --"
# Expected: 21 active, 100% pass

cargo test --workspace 2>&1 | tail -5
# Expected: green, 0 failures
```

Record the actual numbers. Any deviation from the expected baseline is a finding before the audit has even started.

---

## Metrics snapshot — gather first, reference throughout

Run these, record results in `AUDIT_REPORT_2026.md`:

```bash
# Codebase size
find server/ client/ shared/ adapters/ -name "*.rs" | xargs wc -l | sort -rn | head -1
# And top-20 largest files:
find server/ client/ shared/ adapters/ -name "*.rs" -exec wc -l {} \; | sort -rn | head -20

# Panic surface (excluding test code)
grep -rn "todo!()\|panic!()\|\.unwrap()\|\.expect(" \
  server/src/ client/src/ shared/src/ adapters/ \
  | grep -v "#\[test\]\|#\[cfg(test)\]\|test/" \
  | wc -l

# TODO/FIXME/HACK
grep -rn "TODO\|FIXME\|HACK\|XXX" \
  server/src/ client/src/ shared/src/ adapters/ \
  | grep -v "^Binary" | wc -l

# Remaining unsafe (excluding test code)
grep -rn "unsafe " \
  server/src/ client/src/ shared/src/ adapters/ \
  | grep -v "test\|//" | wc -l

# Box<dyn> density
grep -rn "Box<dyn" server/src/ client/src/ shared/src/ | wc -l

# HashMap::new without capacity (potential hot-path allocations)
grep -rn "HashMap::new()" server/src/ client/src/ shared/src/ | wc -l

# Missing doc warnings
cargo doc --workspace --no-deps 2>&1 | grep "^warning" | wc -l

# @Deferred scenarios remaining
grep -rn "@Deferred" test/specs/features/ | wc -l
grep -rn "@PolicyOnly" test/specs/features/ | wc -l
```

---

## Track A — Architectural & Implementation Rigor (G1)

The biggest structural debt items are `WorldServer` (god-object) and `client.rs` (its mirror). Both have been **deferred** but not forgotten. The audit should determine whether the deferral is still correct, or whether the compounding cost now justifies action.

### A.1 God-object audit

**WorldServer** (`server/src/server/world_server.rs`):
- `wc -l server/src/server/world_server.rs` — is it still ~3592 lines?
- `grep -c "^\s*pub fn\|^\s*pub(crate) fn\|^\s*fn " server/src/server/world_server.rs`
- Skim the full method list. Are there any NEW methods added since the last audit that make the decomposition more urgent? Any methods that are clearly in the wrong module?
- Verdict: is D-P2 (decomposition into 10 module files) still correctly deferred, or does the pain now justify scheduling it?

**client.rs** (`client/src/client.rs`):
- Same measurement
- Are the server/client symmetric methods still just mirrors of each other, or have they diverged?
- Does the `Host<E>` abstraction from D-P12 still make sense, or has the code drifted?

### A.2 New code since last audit — correctness check

Read the git log for commits since `release-0.25.0-e` (the last audit baseline). For each commit that touched production code (not just tests or docs), ask: is the change correct and complete?

Specific areas requiring close scrutiny:
- **P1: `give_authority` implementation** — `server/src/server/world_server.rs` + `server/src/server.rs` + bevy wrapper + commands. Does the state machine transition correctly? What happens if `give_authority` is called when the entity is not Delegated?
- **P4: `unpublish_entity` / `publish_entity` bug fix** — read the actual implementation. The fix: (1) non-owner despawn without clearing scope map; (2) deregister from diff handler on unpublish; (3) enqueue EntityEnteredRoom on republish. Is each part implemented? Is there an edge case if the entity is in multiple rooms?
- **P5: `TickBufferedChannel` and `EntityProperty`** — are these test-infrastructure-only, or did they touch production code? If production, read the implementation.
- **P9: Reconnect cleanup** — when a client reconnects, does the server correctly destroy the old connection's state? Specifically: pending authority grants, in-scope entities, room membership, in-flight messages.

### A.3 Invariant discipline

The previous audit found 2206 panic-sites. Re-run the metric (command in §Metrics snapshot).

Focus specifically on:
- `server/src/server/world_server.rs` — was 171 panic-sites. After all the changes, how many remain? Are any new ones `todo!()`?
- `shared/src/world/delegation/entity_auth_status.rs` — T0.1 converted `todo!()` to `unreachable!`. Are the `unreachable!` arms actually documented with the invariant that makes them unreachable?
- Any file with >50 panic-sites: spot-check 5 random `.unwrap()` calls. Is each one genuinely safe, or is it a lazy "I think this is always Some"?

### A.4 Pattern consistency

These should be consistent across the codebase. Flag any violation:

```bash
# Re-exports in non-public crates (should be zero per feedback_no_reexports.md)
grep -rn "pub use" server/src/ client/src/ shared/src/ | grep -v "lib.rs"

# Remaining snake_case package names in Cargo.toml
grep -rn "^name = " server/Cargo.toml client/Cargo.toml shared/Cargo.toml \
  adapters/bevy/server/Cargo.toml adapters/bevy/client/Cargo.toml \
  adapters/bevy/shared/Cargo.toml

# Any println!/eprintln! remaining in production paths
grep -rn "println!\|eprintln!" server/src/ client/src/ shared/src/ adapters/ \
  | grep -v "test\|//\|debug\|demo"
```

### A.5 TODO/FIXME triage

Re-run the TODO count (§Metrics snapshot). For each cluster:
- `shared/src/world/local/local_world_manager.rs` — what do these say? Stale decisions, open questions, or genuine invariant documentation?
- `shared/src/world/delegation/entity_auth_status.rs` — what remains now that the `todo!()` arms are gone?
- Any `// todo!` commented-out arms: are they documented decisions or unresolved questions?

Classify each into: (a) delete (stale), (b) track in NAIA_PLAN.md (open question), (c) convert to `debug_assert!`/`unreachable!` with written invariant.

---

## Track B — API Design & Ergonomics (G3)

A newcomer game developer opens the docs and starts building. Every surprise, every clumsy pattern, every moment of confusion is a finding.

### B.1 Full public API surface — completeness and shape

Generate the surface:
```bash
cargo doc --workspace --no-deps 2>&1 | grep "^warning"
```

For each public crate, open the generated docs and read the top-level module page:
- `naia-shared` — is the public surface minimal?
- `naia-server` — `Server<E>` API: is every method documented? Is the method set minimal (no dead public methods)?
- `naia-client` — same for `Client<E>`
- `naia-bevy-server` — `ServerPlugin`, `ServerCommandsExt`
- `naia-bevy-client` — `ClientPlugin`, `ClientCommandsExt`
- `naia-bevy-shared` — is anything pub that shouldn't be?

Flag: any `pub` item with no doc comment. Any `pub` item that a user would never call directly.

### B.2 Symmetric API check (G1 connection: sender-side symmetry)

Per `feedback_naia_sender_side_framing.md`: the client-authoritative flow needs the same outbound machinery as the server. Check:

Make a table of matching operations:

| Operation | Server API | Client API | Symmetric? |
|---|---|---|---|
| Send message | `server.send_message` | `client.send_message` | ? |
| Request/grant authority | `server.give_authority` | `client.request_authority` | ? |
| Entity spawn | `server.spawn_entity` | N/A (server only) | — |
| Resource replicate | `server.replicate_resource` | N/A? | — |
| Resource authority | `server.configure_resource_authority` | `client.request_resource_authority` | ? |

For each pair: are the parameter orders consistent? Are the return types consistent? If the server version panics on bad input but the client version returns an error, that's a finding.

### B.3 Error handling at API boundaries

What can go wrong when a user calls the public API, and how does naia communicate it?

For each of these, trace what happens on a bad call:
- `client.request_authority(entity)` — entity not in scope
- `server.give_authority(user_key, entity)` — entity not Delegated
- `server.send_message(user_key, channel, msg)` — user disconnected
- `commands.enable_replication()` — called twice on same entity
- `server.replicate_resource::<R>()` — called before R is registered

Are the failure modes panics? Silent no-ops? `Result::Err`? The most dangerous failure mode is a **silent no-op** — bad input is accepted with no feedback, causing a debugging nightmare downstream.

### B.4 Bevy ergonomics

Focus on what a Bevy game developer actually experiences:

- The `T: Send + Sync + 'static` client-tag generic: is it documented clearly? Will a user understand why it exists?
- `add_resource_events::<T, R>()` vs `add_resource_events::<R>()` disambiguation: we hit this during T4.1-T4.3. Is this exposed to users? If so, document it prominently or fix it.
- Does the Bevy plugin system ordering work correctly? (ReceivePackets → ... → SendPackets). Is the ordering documented somewhere users can find it if they add custom systems?
- `ClientCommandsExt` vs `ServerCommandsExt`: do they have the same method naming conventions? Are there any naming collisions that would cause UFCS disambiguation issues for users?

### B.5 Feature flag UX

From P11.5 audit:
- `transport_local`, `test_time`, `transport_webrtc`, `wbindgen`, `mquad`, `interior_visibility`, `test_utils`, `bench_instrumentation`
- Is the **default feature set** correct for a new user? (What happens with just `naia-server = { version = "..." }` with no explicit features?)
- Are `test_time` and `test_utils` correctly documented as "only for testing"?
- Is `transport_webrtc` clearly documented as the production transport?

---

## Track C — Performance & Scalability (G4)

This is where Naia needs to be not just fast, but *demonstrably* fast. The goal is benchmarks that make people do a double-take.

### C.1 Hot-path allocation audit

Every allocation in the tick loop is a potential perf cliff at scale.

The tick loop lives primarily in:
- `server/src/server/world_server.rs` — the main `receive_all_packets` / `send_all_packets` loop
- `shared/src/world/update/mut_channel.rs` — dirty queue processing
- `shared/src/connection/base_connection.rs` — per-packet processing

For each of these files, search for:
```bash
grep -n "Vec::new\|HashMap::new\|BTreeMap::new\|Box::new\|Arc::new\|String::new\|vec!\[\|\.clone()" \
  server/src/server/world_server.rs \
  shared/src/world/update/mut_channel.rs \
  shared/src/connection/base_connection.rs
```

For each hit: is this in a hot path (called per-tick or per-entity)? If yes: can it be pre-allocated, stack-allocated, or avoided entirely?

### C.2 HashMap capacity audit

`HashMap::new()` hits re-run: `grep -rn "HashMap::new()" server/src/ client/src/ shared/src/ | wc -l`

For each `HashMap::new()` hit:
- Is it in a hot path or a one-time initialization?
- If hot path: should it be `HashMap::with_capacity(expected_size)`?
- Key: per-connection `HashMap`s that grow and shrink with entity/user count need careful capacity management at the Cyberlith target of 1262 CCU.

### C.3 Vtable density audit

`Box<dyn>` count from §Metrics snapshot.

The high-value targets (where vtable dispatch matters most):
- `Box<dyn ReplicateBuilder>` in `ComponentKinds::kind_map` — looked up on every component decode. At 70 component kinds, this is 70 vtable dispatches per entity per tick. Could this be a per-kind dispatch table (array-indexed)?
- `Box<dyn Replicate>` in `remote_world_manager.rs`'s `incoming_components` — evaluated on every received packet.
- Any `Box<dyn ...>` in the priority accumulator or dirty queue hot paths.

For each: estimate the call frequency at scale (1262 CCU × 10K entities). If the vtable dispatch is in the top-3 hot paths under profiling, flag for enum dispatch investigation.

### C.4 Benchmark coverage gap analysis

Read `_AGENTS/BENCHMARKS.md` for the full bench suite.

The current flagship bench is `halo_btb_16v16`. What's NOT covered:

| Scenario | Benched? | Value |
|---|---|---|
| Single-client round-trip latency | ? | User-perceived experience |
| Resource replication throughput | ? | New flagship feature |
| Authority grant/revoke cycle cost | ? | D-P1 just implemented give_authority |
| OrderedReliable vs Unordered throughput delta | ? | Channel selection guidance |
| Large message fragmentation | ? | Message size limits |
| 10K entity scope entry (batch) | Claimed (Win 1) | Verify still holds |
| Dirty-bit mutation scaling (K mutations, N entities) | Claimed (Win 3) | Verify still holds |
| Reconnect overhead | ? | P9 added reconnect; is it fast? |

For each uncovered scenario: propose whether it should be added to the bench suite, and if so, what claim it would verify.

### C.5 Protocol wire efficiency

The bit-encoding layer is a direct performance lever.

- Review the bit-level encoding for component kind tags. After the 64-kind limit removal (T1.3), kind tags are variable-width based on actual kind count. Is the tag width optimal? (E.g., for 10 kinds, are we using 4 bits not 8?)
- Entity ID encoding: how many bits per entity ID on the wire? Is there a variable-length encoding opportunity?
- Is there any fixed-size overhead in packet headers that could be shrunk for small packets?
- Does the priority accumulator correctly prioritize high-frequency mutations over low-frequency ones? Could the Fiedler pacing be tuned further?

### C.6 Scalability ceiling

Using `naia/_AGENTS/CAPACITY_ANALYSIS_2026-04-26.md` as context:

The stated ceiling is ~1262 CCU per server. What is the **theoretical bottleneck** — is it:
- CPU (tick loop per entity)
- Memory (per-client dirty queues × entity count)
- Bandwidth (packet budgets)
- Single-thread (is `WorldServer` single-threaded? Can it be parallelized?)

For the identified bottleneck: what's the headroom before it bites at 1262 CCU? Is there a specific code path that would become the limiting factor?

### C.7 Benchmark integrity check

Run: `cargo run --manifest-path benches/Cargo.toml -- --assert` (or equivalent per `_AGENTS/BENCHMARKS.md`)

Expected: 29/0/0 (wins/ties/losses). Does it still hold? If any regressions have crept in since the perf upgrade phases, find and fix them.

Also verify: is the `iai-callgrind` baseline still current? Are there instruction-count benchmarks that have drifted up without anyone noticing?

---

## Track D — Test Coverage (G2)

The goal is not line coverage — it's **behavioral coverage**. Every promise the naia protocol makes should be verified by at least one test.

### D.1 BDD scenario gap analysis

Run the full scenario list:
```bash
namako lint --specs-dir test/specs 2>&1 | grep "Scenario\|Rule" | head -100
```

Walk through each major protocol area and ask: "What edge case isn't exercised?"

**Entity lifecycle:**
- Spawn → scope in → despawn: covered
- Spawn → scope in → scope out (disable_replication): covered
- Spawn → publish → unpublish → republish: covered (entity-publication-11)
- Entity migration between rooms while in scope: covered? If not, should be.
- Entity that is in scope for client A, migrated to a room client B is in, but not client A: covered?

**Authority:**
- Server gives authority (P1): covered
- Client requests authority: covered
- Contended requests (first-wins): covered (Bevy NPA)
- Authority during reconnect (connection-35): covered
- Authority released cleanly before reconnect: covered?
- Server revokes authority while client is mutating: covered?

**Resources:**
- Replicate → mutate → observe: covered
- Authority delegation → client holds → observe Denied on server: covered
- Resource during reconnect (connection-34): covered
- `remove_resource` while client holds authority: covered?
- Two clients requesting resource authority (contended): covered?

**Messaging:**
- OrderedReliable ordering guarantee: covered?
- SequencedReliable (latest-wins): covered?
- Tick-buffered delivery: covered (P5)
- Messages during disconnect: covered?
- Messages to disconnected client (what happens?): covered?

**Reconnect:**
- Entity re-enters scope: connection-33, covered
- Resource replicated to new session: connection-34, covered
- Authority available on reconnect: connection-35, covered
- Rapid reconnect (connect-disconnect-connect in < 1 tick): covered?

### D.2 @Deferred status audit

```bash
grep -rn "@Deferred" test/specs/features/ -A 3
```

For each @Deferred scenario: has any infrastructure added in P1-P11 now made it testable? If so, convert it. The harness now supports:
- Reconnect sequences
- Tick-buffered messages
- EntityProperty buffers
- Named clients (ClientName vocab)
- Multi-client authority contention (though `_CANNOT` list says "single-threaded" — verify if this is still a genuine limit)

### D.3 Harness capability gaps

From `TEST_INFRA_PLAN.md`, these were listed as things the harness cannot do. Are any of them now worth adding?

- **Dynamic component ops** (insert/remove component on already-scoped entity): Is there a scenario that needs this?
- **Server-side MessageEvent tracking**: Any @Deferred scenario that needs it?
- **Server-side AuthDenied event**: connection-35 tests this from client side; does the server perspective need a step?
- **Static entity replication** (`as_static()` / `enable_static_replication()`): Are there @Deferred scenarios blocked on this?
- **Per-component packet inspection/drop**: Would this unlock any important adversarial tests?

For each: assess whether adding it would unlock new scenarios, and if those scenarios test important protocol guarantees.

### D.4 Property-based testing (D-P9.A3)

This was deferred in favour of BDD. Reassess:

- **What does proptest add that BDD cannot?** Specifically: message ordering under arbitrary packet sequences, authority state machine under arbitrary interleavings.
- For `OrderedReliable`: a proptest that generates random packet drop/reorder patterns and asserts messages are delivered in-order would prove the guarantee far more thoroughly than any hand-written scenario.
- For the authority state machine: a proptest that generates random request/grant/revoke sequences would prove the state machine has no invalid transitions.
- Verdict: is it time to schedule D-P9.A3, or is BDD coverage sufficient?

### D.5 Adversarial correctness

What happens when a client sends unexpected or malformed protocol traffic? This is important for production security.

- Malformed packet: does the server panic, disconnect the client cleanly, or silently corrupt state?
- Client sends mutation for an entity it has no authority over: is this checked? At what layer?
- Client requests authority for an entity not in its scope: is this rejected?
- Client sends a message on a channel it's not registered for: what happens?

These don't necessarily need BDD scenarios — some are protocol-level invariants that should be `unreachable!` with documented justifications. But the audit should verify the protection exists at each layer.

### D.6 Bevy NPA coverage

Current: 21 scenarios (T4.1-T4.3 complete).

Are there Bevy-specific behaviors NOT covered?
- The Bevy system ordering guarantee (ReceivePackets → SendPackets): is this tested?
- Bevy `Commands` deferral (changes applied next frame): are there scenarios where the one-frame delay matters?
- `bevy_time` vs `TestClock` interaction: any edge cases in the tick triggering?
- Multiple Bevy plugins registered in the same app: any conflicts?

---

## Track E — Documentation Quality (G5)

D-P10 was completed in the previous session (2026-05-08). This track **spot-checks quality and accuracy**, not coverage. The question is: is the documentation actually good, not just present?

### E.1 README

Read the full README with fresh eyes.
- Can a newcomer understand what naia is in 30 seconds?
- Are the install instructions minimal and correct?
- Is the Bevy version badge current?
- Does the architecture section accurately describe the wire protocol, replication model, and authority system?
- Are the demo run commands correct and still functional?

### E.2 CONCEPTS.md — depth check

Read `docs/CONCEPTS.md`.
- Does it explain the entity-component replication model clearly, with an example?
- Does it explain the authority model: the Available → Requested → Granted → Releasing cycle, the Delegated config, and what Denied means from each perspective?
- Does it explain the Bevy integration: why `T` (the client tag) exists, how `Res<R>` is populated, how `CommandsExt` extends Bevy's `Commands`?
- Are there any concepts that required significant debugging during T4.1-T4.3 that aren't explained? (Specifically: `TestClock`, `add_resource_events` UFCS, room membership for resource scope — these were non-obvious.)

### E.3 API doc spot-check

Pick these 6 items and read their `///` docs carefully:

1. `Server::send_message` — does the doc explain channels, delivery guarantees, what happens if user is disconnected?
2. `Client::request_authority` — does it explain the state transition? What does `Delegated` config mean?
3. `ServerCommandsExt::give_authority` — does it document the precondition (entity must be Delegated)?
4. `ClientCommandsExt::request_resource_authority` — does it explain what "Denied" means on the server side?
5. `ProtocolBuilder::tick_interval` — does it explain the interaction with `TestClock` in test code?
6. The `Replicate` derive macro — does the doc show a minimal usage example?

For each: is the doc accurate, complete, and actionable? Would a developer reading only the doc understand how to use the item correctly?

### E.4 CHANGELOG accuracy

Read `CHANGELOG.md`.
- Does it include entries for: Perf upgrade (Phases 0-10), Replicated Resources, Priority accumulator, entity-authority `give_authority`, the publication bug fix, the reconnect edge cases?
- Is the version attribution correct? (What's in `release-0.25.0-e` vs `dev`?)

### E.5 Demo code quality

Read one demo end-to-end (`demos/bevy/` is preferred — most users will hit this first):
- Does the demo code use the API correctly? (No antipatterns that would teach bad habits.)
- Does it exercise the main features: entity replication, authority delegation, messaging?
- Is it commented/documented well enough for a newcomer to understand the flow?
- Does it actually compile and run? (Try it.)

---

## Track F — Protocol Correctness (Deep Dive, G1)

The networking guarantees naia advertises must actually hold. This track verifies the correctness of the most critical protocol mechanisms.

### F.1 Message ordering — sequence number correctness

`shared/src/connection/base_connection.rs` is the heart of the ordering machinery.

- How are sequence numbers assigned? Is there a risk of aliasing (same sequence number for two different messages before the first is acknowledged)?
- Is u16 wraparound handled? At 60 ticks/sec, a u16 wraps every ~18 minutes. The comparison `seq_a < seq_b` wraps incorrectly without explicit handling.
- For `OrderedReliable`: if packet N is dropped, all subsequent packets are held until N arrives (head-of-line blocking). Is there a timeout that disconnects the client if N never arrives? Or can a client hang the connection indefinitely?
- For `SequencedReliable`: if packet N arrives after N+1, N should be discarded. Is this implemented correctly at the boundary?

### F.2 Authority state machine completeness

Draw the full authority state machine from the code, not from the docs.

Start at `shared/src/world/delegation/entity_auth_status.rs` and `server/src/world/server_auth_handler.rs`.

States: Available, Requested, Granted, Releasing, Denied.

For each state transition:
- What triggers it?
- What code implements it?
- Is there a guard that prevents invalid transitions?
- What happens if the trigger arrives in an unexpected state?

Specific edge cases to trace:
- Client disconnects while in `Requested` state. Does the server clean up and return to `Available`?
- Client disconnects while in `Granted` state. Does the server return to `Available` (P9 covers this for reconnect, but verify the general disconnect path)?
- Two clients both in `Requested` simultaneously. Which gets `Granted`? Is the tie-breaking deterministic?
- Server calls `give_authority` while the entity is already in `Granted` for a different client. What happens?

### F.3 Scope management edge cases

`server/src/server/world_server.rs` scope management.

- What happens if `scope_entity_for_user` is called twice for the same entity-user pair?
- What happens if an entity is despawned while its scope notification is in the pending queue for a client?
- What happens if a user leaves a room while there are pending scope-change events for entities in that room?
- Is there any TOCTOU race between checking entity existence and acting on it? (Single-threaded: probably no, but verify.)

### F.4 Resource replication edge cases

- What happens if `remove_resource::<R>()` is called while a client holds authority for R?
- What happens if the server mutates a resource while a client holds authority? Is the server's mutation blocked, silently dropped, or do both sides diverge?
- What happens if `replicate_resource::<R>()` is called twice? Panic? Silent no-op? Second registration wins?

### F.5 Reconnect protocol correctness (P9 audit)

P9 added connection-33/34/35. Verify the underlying protocol is correct, not just that the scenarios pass.

- When a client disconnects and reconnects with the same key (or a new key?): does the server correctly destroy the old `Connection` object and all its state?
- In-scope entities from the old connection: are they removed from scope on disconnect? Or do they re-enter scope on reconnect?
- Pending authority grants from the old connection: are they cleaned up or transferred?
- The new connection's first tick: are there any ordering assumptions that break if the reconnected client's first packet arrives out-of-order?

---

## Audit Process

### How to conduct each track

1. **Gather metrics first** (§Metrics snapshot above) before reading any code.
2. **Read code at specific files**, not by random exploration. The §Key files list below anchors each track.
3. **Form verdicts**, not observations. "This file is long" is an observation. "This file should be decomposed as follows, because..." is a verdict.
4. **Record findings** in `AUDIT_REPORT_2026.md` as you go, not at the end.

### Finding severity

| Mark | Meaning | Action |
|---|---|---|
| 🚨 | Latent bug, production-reachable panic, or broken protocol guarantee | Fix before closing audit |
| ⚠️ | High-impact architectural debt with compounding cost | Schedule as active phase in NAIA_PLAN.md |
| 💡 | Refactor opportunity with clear but non-urgent value | Add as deferred phase in NAIA_PLAN.md |
| 🔬 | Test coverage gap for an important protocol guarantee | Add scenario or convert @Deferred |
| 📚 | Documentation inaccuracy or gap | Fix inline or add to deferred |
| ✅ | Explicitly checked, found acceptable | Record the check and result — absence of a finding is also evidence |

### Key files to read in every audit

Any audit that doesn't look at these is incomplete:

| File | Why |
|---|---|
| `server/src/server/world_server.rs` | God-object, most production-critical file; largest source of risk |
| `client/src/client.rs` | Mirror of WorldServer; 2311 lines, 95 methods |
| `shared/src/world/component/replicate.rs` | Core protocol trait, 29 methods; complexity hub |
| `shared/src/world/delegation/entity_auth_status.rs` | Authority state machine; T0.1 landing zone |
| `shared/src/connection/base_connection.rs` | Message ordering, sequence numbers; protocol correctness ground truth |
| `shared/src/world/update/mut_channel.rs` | Dirty queue; hot path for all component mutation tracking |
| `adapters/bevy/server/src/systems.rs` | Bevy system ordering; correctness + ergonomics intersection |
| `adapters/bevy/client/src/systems.rs` | Client-side Bevy system ordering |
| `shared/src/world/local/local_world_manager.rs` | TODO concentration; scope management |
| `server/src/world/server_auth_handler.rs` | Authority grant/deny/revoke logic |

---

## Deliverables

The audit produces two artifacts:

### 1. `_AGENTS/AUDIT_REPORT_2026.md`

A new document, written as the audit proceeds, containing:
- The §Metrics snapshot values (gathered at audit start)
- Per-track findings, each with: file:line, severity mark, description, verdict
- A summary scoreboard (like the per-area scorecards in `CODEBASE_AUDIT.md`)
- A priority-ranked list of all 🚨/⚠️ findings for immediate action

### 2. Updated `_AGENTS/NAIA_PLAN.md`

After the report is complete:
- Update the **§Current state snapshot** table with fresh metrics
- Add new **active phases** for any 🚨/⚠️ findings
- Add new **deferred phases** for 💡/🔬/📚 findings that are real but non-urgent
- Update §Acceptance criteria if the bar should be raised

Commit both artifacts to `dev` and push.

---

## Bottom line

This audit is not about finding problems for their own sake. It's about holding Naia to the standard it claims: a production-grade, high-performance, elegant networking library for real games. Every finding should answer the question: *"Does this keep us from being the best Rust game networking library in existence?"* If yes, it's a real finding. If not, it's noise.

The codebase has improved dramatically since the last audit. The goal now is to find what's left between "very good" and "exceptional."
