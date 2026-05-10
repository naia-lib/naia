# Naia — Codebase Audit Plan V2

**Purpose:** Systematic, objective audit of the naia networking library for production readiness.  
**Scope:** All production crates — `server/`, `client/`, `shared/`, `adapters/`, `socket/`.  
**Excludes:** `test/`, `benches/`, `demos/` except where they reveal gaps in the library itself.  
**Auditor stance:** Fresh eyes. Do not anchor on prior plans, prior audits, or prior conversations. Read the code as it is today.

---

## How to run this audit

Work through each track in order. For each item:
1. **Investigate** — read the relevant code; run the suggested commands
2. **Classify** each finding as: `OK` / `MINOR` / `NOTABLE` / `CRITICAL`
3. **Record** findings in `AUDIT_REPORT_V2.md` under the matching track heading
4. **Propose** a concrete fix for anything `NOTABLE` or `CRITICAL`

Do not skip tracks. Do not stop early. A clean bill of health is a valid outcome.

---

## Track A — API Design & Ergonomics

*Goal: A user picking up naia for the first time should find the API intuitive, consistent, and hard to misuse.*

**A.1 — Naming consistency**  
Scan all `pub fn` names across `Server`, `Client`, and the Bevy adapter. Are method names consistent for the same concept (e.g., does "spawn" always mean the same thing)? Do similar operations have parallel names on client and server? Flag asymmetries.

**A.2 — Fallibility**  
Count public methods that return `()` vs `Result<_, _>`. For every void-returning mutation method, ask: can this silently fail? If yes — is the failure surfaced elsewhere (events)? Is that documented? Flag any case where failure is silently swallowed with no way for the caller to detect it.

**A.3 — Footguns**  
Look for methods that panic in plausible misuse scenarios (calling a method before connection is established, using an entity key after despawn, etc.). Is every panic reachable by correct library usage, or only by violating a documented precondition? Flag panics that can be triggered by plausible mistakes.

**A.4 — Builder / config ergonomics**  
Review `ServerConfig`, `ClientConfig`, `ConnectionConfig`, `Protocol`, `ChannelConfig`. Are defaults sensible for a typical game? Are misconfiguration combinations caught at construction time or only at runtime? Is the minimum-viable setup documented?

**A.5 — Generic bounds**  
Review the `E: Copy + Eq + Hash + Send + Sync` bound that appears everywhere. Is it documented why each bound is required? Are there methods where the bound is unnecessarily restrictive?

---

## Track B — Error Handling

*Goal: Errors should be informative, recoverable where possible, and never silently lost.*

**B.1 — Error type coverage**  
List all error enums/structs in the codebase. For each: how many variants? Are the variants specific enough to act on, or are they catch-all wrappers? Is `Display` implemented usefully?

**B.2 — Panic vs Result**  
Grep for `unwrap()`, `expect(`, `panic!(`, and `unreachable!()` outside of test code. For each site: is the invariant that makes this safe documented? Could a library consumer ever trigger this panic through normal (even mistaken) usage? Classify each site.

**B.3 — Event-based error surfacing**  
Naia surfaces some errors through events rather than return values. Is this pattern consistent? Are all error paths reachable via events? Is there any error that a consumer would have no way to observe?

**B.4 — Error granularity in transport**  
Review the transport send/receive error types. Is a connection error distinguishable from a serialization error from a capacity error? Can a consumer take different recovery actions for each?

---

## Track C — Safety & Soundness

*Goal: All unsafe code must be justified and minimal.*

**C.1 — Unsafe inventory**  
Grep for `unsafe` in all production code. List every site. For each: what safety invariant is being relied on? Is there a `// Safety:` comment? If not, add one as a finding.

**C.2 — Transmute and lifetime extension**  
Pay special attention to any `transmute` or lifetime-erasure pattern. Is the extended lifetime actually valid for the entire period it's used? Could the underlying data be freed while the extended reference exists?

**C.3 — Manual `Send`/`Sync` impls**  
For every `unsafe impl Send` or `unsafe impl Sync`: what type is this on? Why can't the compiler derive it? Is the manual impl correct (i.e., is the type genuinely safe to send across threads)?

**C.4 — FFI and extern blocks**  
Any `extern "Rust"` or `extern "C"` blocks: are they only active under feature flags? Are the signatures correct? Is there a risk of calling them in the wrong context?

---

## Track D — Documentation

*Goal: Every public item has a doc comment. Doc comments are accurate, not aspirational.*

**D.1 — Public API coverage**  
For the primary public types (`Server`, `Client`, and their key methods), what fraction have `///` doc comments? Anything missing a doc is a finding.

**D.2 — Accuracy**  
Pick 10 documented methods at random and verify the doc matches the implementation. Look especially for: documented panics that no longer exist, missing documented panics, wrong parameter descriptions, stale cross-links.

**D.3 — Crate-level docs**  
Review the `//!` blocks in each `lib.rs`. Do they give a newcomer a correct mental model? Are there outdated references to removed features?

**D.4 — Safety comments on unsafe**  
Every `unsafe` block should have a `// Safety:` comment. Any that don't is a `NOTABLE` finding.

**D.5 — Example correctness**  
In `demos/`, does each demo compile? Does it demonstrate idiomatic usage? Are there patterns in the demos that conflict with what the docs say?

---

## Track E — Test Coverage

*Goal: The test suite should cover the things most likely to break silently.*

**E.1 — BDD spec completeness**  
Read the feature files in `test/specs/features/`. Are there observable behaviors of the library that have no corresponding scenario? Focus on edge cases: what happens at connection boundary (just before connect, just after disconnect)? What happens when the same operation is called twice? What happens under packet loss?

**E.2 — Deferred scenarios**  
List all `@Deferred` scenarios. For each: is it deferred because it's untestable, or because it was never implemented? Are any of these covering important behavior?

**E.3 — Error path coverage**  
Are error paths tested? Specifically: malformed packets, auth failures, capacity exhaustion, entity-not-found conditions. BDD tends to cover happy paths; check for adversarial coverage.

**E.4 — Reconnection**  
Is the reconnect flow (client disconnects and reconnects to the same server) tested? Are there scenarios covering server-initiated disconnect vs client-initiated disconnect? Is the client state clean after disconnect?

**E.5 — Property / fuzz gaps**  
Are there any stateful or numerical properties that would benefit from property-based testing (e.g., sequence number wrapping, priority accumulation, jitter buffer behavior under varied latency)? Note gaps; don't implement.

---

## Track F — Dependency Health

*Goal: Dependencies should be current, maintained, and minimal.*

**F.1 — Direct dependency audit**  
List all direct dependencies of `naia-server`, `naia-client`, and `naia-shared`. For each: is it actively maintained? Is the version current? Is it actually used (not just declared)?

**F.2 — Security advisories**  
Run `cargo deny check advisories` (or inspect `deny.toml` ignores). List all active ignores. For each: what is the advisory? Is the ignore justified? When does it expire?

**F.3 — Duplicate dependencies**  
Run `cargo tree --duplicates`. List any crates with multiple versions in the dependency tree. Are these duplicates avoidable?

**F.4 — Feature bloat**  
For each optional dependency, is it actually optional in the way claimed? Are there features that pull in heavier deps than expected?

---

## Track G — Protocol Correctness

*Goal: The wire protocol must be correct, complete, and resilient to malformed input.*

**G.1 — Deserialization hardening**  
Trace the path from `io.recv_reader()` to the point where a packet's contents are acted on. At each parse step: what happens if the bytes are truncated? Malformed? Contain out-of-range enum discriminants? Is there a `return` / `continue` / graceful discard, or a panic / unwrap?

**G.2 — State machine completeness**  
Identify all state machines in the protocol (connection state, entity authority state, entity delegation state, entity publish state). For each: are all transitions explicit? Are illegal transitions impossible (type system), panicking (runtime), or silently ignored? Silently ignored is a finding.

**G.3 — Sequence number handling**  
Naia uses wrapping sequence numbers. Find all arithmetic on these numbers. Is wrapping handled correctly? Is there a test for wrap-around? Could a large gap in sequence numbers cause incorrect behavior?

**G.4 — Tick buffer correctness**  
The tick buffer holds client inputs for replay. What happens if the client sends a message for a tick that has already passed? For a tick far in the future? For a duplicate tick? Are these cases handled explicitly?

**G.5 — Authority state consistency**  
The entity authority state machine is complex. Read `entity_auth_status.rs` and the server/client authority handlers together. Is there any sequence of events that could leave client and server with inconsistent authority beliefs? Is this covered by a BDD scenario?

---

## Track H — Performance & Allocation

*Goal: The hot path (per-tick send/receive loop) should not allocate unnecessarily.*

**H.1 — Hot path allocation survey**  
Trace `send_all_packets` and `process_all_packets` / `receive_all_packets` in both client and server. At each step, note: Vec allocations, Box allocations, String formatting, clone() calls. Flag any that occur unconditionally per-tick.

**H.2 — Broadcast allocation**  
When the server broadcasts a message to N clients, is the message body shared (Arc / ref) or cloned N times? Verify the current state and flag if any broadcast path still clones.

**H.3 — Scope check scaling**  
The scope check system runs every tick. What is its time complexity as a function of (users × entities)? Is there any caching or early-exit that limits worst-case work? Flag if the complexity is super-linear.

**H.4 — Priority accumulator**  
The priority system accumulates priority for entities not yet sent. Is there a bound on how large these accumulators get? Is old priority data cleaned up when entities leave scope?

---

## Track I — Configuration & Limits

*Goal: Limits should be explicit, documented, and defensively enforced.*

**I.1 — Magic numbers**  
Grep for raw numeric literals (u8::MAX, 1024, 65535, etc.) outside of test code. For each: is it a documented limit? Is it named? Should it be configurable?

**I.2 — Unbounded collections**  
Identify any `Vec`, `HashMap`, or `VecDeque` that grows in response to external input (incoming packets, connecting clients, spawning entities). Is there a cap? What happens when the cap is hit — or if there is no cap?

**I.3 — Timeout and interval defaults**  
Review all `Duration` defaults in config structs. Are they documented with the rationale for the chosen value? Are they appropriate for both LAN (low latency) and WAN (high latency, lossy) conditions?

---

## Track J — Code Quality & Maintainability

*Goal: The codebase should be easy to reason about and extend.*

**J.1 — TODO / FIXME density**  
Grep for `TODO`, `FIXME`, `HACK`, `XXX` in production code. For each: is it tracking a known correctness issue, a known performance issue, or a vague aspiration? Correctness-related TODOs are `NOTABLE` or `CRITICAL`.

**J.2 — Dead code**  
Grep for `#[allow(dead_code)]`. For each: is the code actually used externally (re-exported for adapters)? Or is it genuinely dead? Dead code that isn't re-exported should be removed.

**J.3 — `allow(clippy::...)` suppressions**  
List all clippy suppression attributes. For each: is the suppression justified? Is the underlying complexity a genuine design constraint or an accidental complexity that could be simplified?

**J.4 — Largest files**  
Find the 10 largest `.rs` files by LOC. For each: is the size justified (e.g., a complex protocol state machine), or is the file doing too many things? Flag any file that feels like it should be split.

**J.5 — Comment quality**  
For the most complex methods (scope update, authority handling, entity delegation), do the comments explain *why*, or only *what*? A comment that just restates the code is not useful.

---

## Track K — Security

*Goal: A game server using naia should not be trivially deniable or exploitable.*

**K.1 — Auth path hardening**  
Trace the authentication path for both the advanced handshaker and simple handshaker. What happens with: a connection that never sends auth? An auth message that is too large? An auth message with invalid encoding? A replayed auth token?

**K.2 — Input validation on packet receive**  
For every packet type that the server processes from a client: is there a maximum length check? Is every field range-checked before use? Is any field used as an index into an array without bounds checking?

**K.3 — Amplification**  
Could a small client packet cause the server to do a disproportionate amount of work (e.g., a single packet triggering O(N users) processing)? Flag any such paths.

**K.4 — Resource exhaustion**  
Can an unauthenticated or misbehaving client cause unbounded memory growth on the server? Consider: connection slots, message queues, tick buffer entries, entity spawns from a client-authoritative client.

---

## Deliverables

Write findings to `_AGENTS/AUDIT_REPORT_V2.md`. Structure:

```
# Naia — Audit Report V2
**Date:** <date>
**Auditor:** <model/session>
**Gate at audit time:** <namako gate output summary>

## Track A — [findings or "No findings"]
...
## Track K — [findings or "No findings"]

## Summary
| Track | OK | MINOR | NOTABLE | CRITICAL |
|---|---|---|---|---|
...

## Recommended action list (NOTABLE + CRITICAL only, prioritized by impact)
```

A finding with no recommended fix is not complete. Every `NOTABLE`/`CRITICAL` must have a proposed concrete action.
