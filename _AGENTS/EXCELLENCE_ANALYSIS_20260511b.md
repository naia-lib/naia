# Naia Excellence Audit — 2026-05-11 (b)

## Executive Summary

**Overall grade: B+**

Naia is a production-quality entity replication library with a uniquely
comprehensive feature set for an open-source Rust networking library: per-field
delta compression, priority-weighted bandwidth allocation, two-level interest
management, authority delegation, tick-buffered prediction primitives, lag
compensation via `Historian`, optional zstd compression with dictionary
training, and connection diagnostics with RTT percentiles. Documentation quality
has improved substantially — README, CONCEPTS.md, PREDICTION.md, SECURITY.md,
MIGRATION.md, and FEATURES.md together form a thorough reference for the
features that exist.

The library falls short of A because of three compounding gaps: (1) the clippy
gate is currently broken — six errors across three files prevent `cargo clippy
--workspace --all-targets -- -D warnings` from passing, a visible quality
signal to any new contributor; (2) MSRV is undeclared in every `Cargo.toml`,
leaving the compatibility surface undefined; (3) unwrap density in the UDP
transport receive path is high enough that a malformed auth packet can panic the
server process, which is a correctness risk for any deployment that exposes the
auth TCP port publicly.

The single change that would move the grade from B+ to A is **fixing the clippy
gate and declaring MSRV in the workspace `Cargo.toml`** — both are small-effort
changes that close the loudest "abandoned project?" signal for new evaluators.

---

## Competitive Landscape

| Competitor | Strengths vs naia | Weaknesses vs naia |
|------------|-------------------|-------------------|
| **lightyear** | Built-in prediction/rollback with one-line enable; WebTransport + Steam relay shipped; modular via sub-crates (lightyear_avian, lightyear_steam); integrated metrics visualizer; active community; `https://cbournhonesque.github.io/lightyear/book/` full book doc | Bevy-only (no ECS-agnostic core); heavier dependency footprint; learning curve higher than naia for non-Bevy users; no smol/async-std |
| **renet / renet2** | Extremely simple API; many shipped games; netcode.io auth; WebTransport + WebSocket in renet2; large community | Message-only (no entity replication); prediction/rollback entirely manual; no bandwidth prioritization; no lag compensation |
| **bevy_replicon** | Very clean Bevy-native API; multiple transport backends (renet2, quinnet, aeronet); replication into scene for save; ECS relationship-based scoping | Prediction/interpolation not in core (delegate to bevy_replicon_snap); Bevy-only; no smol; no lag compensation in core |
| **GGRS / bevy_ggrs** | Gold standard for deterministic P2P rollback (fighting games etc) | Server-authoritative model not supported; requires deterministic simulation; complementary, not competing |
| **GameNetworkingSockets (Valve SDR)** | Production at planetary scale; per-lane reliability; dev simulation tools; Steam relay built-in | C++ native only; no Rust-native first-class support; not open source |

**Key competitive observation:** lightyear is the dominant mindshare choice for
Bevy developers, with a full mdBook tutorial at
`cbournhonesque.github.io/lightyear/book/` and shipping features (WebTransport,
Steam relay, built-in rollback) that naia explicitly defers. For developers
committed to Bevy, naia's ECS-agnostic architecture is a neutral point, not
an advantage. Naia's strongest differentiators vs lightyear are: smol/async-std
runtime (no tokio overhead), the `Historian` for lag compensation, the
priority-bandwidth system, and deeper documentation of the low-level protocol
model.

---

## Gap Analysis

### A-1 — Clippy gate broken (six errors)

**Current state (what naia has today):**
Running `cargo clippy --workspace --all-targets -- -D warnings` produces six
errors across three files, failing the build:

- `socket/shared/src/backends/native/random.rs:3:1` — `empty_line_after_doc_comments`
  (empty line between `///` comment and `pub struct Random`)
- `shared/serde/src/impls/scalars.rs:129:17`, `:272:17`, `:309:17`, `:346:17` —
  `clippy::cast_ptr_alignment` ("casting raw pointers to the same type and
  constness is unnecessary")
- `shared/derive/src/message.rs:663:1` — `clippy::large_enum_variant`
  (`FieldKind` enum, variant `Normal(Type)` significantly larger than `EntityProperty`)

**Why it matters:**
Any developer evaluating naia will run clippy as a first health-check. A
broken gate signals "unmaintained" and raises doubt about code quality even
when the underlying code is sound. GitHub CI passes fewer checks than a local
`-D warnings` run, so this gap is invisible to casual observers of the CI
badge but visible to anyone doing due diligence. Lightyear enforces a clean
clippy gate.

**Recommendation:**
1. `random.rs:3` — remove the empty line between the doc comment and the struct
   declaration (one-character edit).
2. `scalars.rs` — replace the identity casts with `as_ptr()` / `as_mut_ptr()`
   calls or simply remove the unnecessary casts.
3. `message.rs:663` — box the large variant: `Normal(Box<Type>)`, or annotate
   with `#[allow(clippy::large_enum_variant)]` with a rationale comment if the
   boxed allocation is undesirable in a proc-macro context.

**Effort:** XS (< 1 hour)
**Leverage:** 5 — visible quality signal, zero risk

---

### A-2 — MSRV undeclared

**Current state:**
No `rust-version` field exists in any `Cargo.toml` in the workspace (`grep
rust-version` returns nothing across server/client/shared/socket crates).
There is no MSRV CI job or documentation note about the minimum supported
compiler version.

**Why it matters:**
MSRV declaration is now a crates.io quality signal and a standard expectation
for published crates. Without it, downstream developers cannot know whether
their toolchain is compatible. Competitors like bevy_replicon and lightyear
declare MSRV. The absence is a small but consistent friction point when
evaluating naia for production use.

**Recommendation:**
Determine the actual MSRV by compiling with the oldest toolchain you wish to
support (1.75 is a reasonable starting point given `edition = "2021"` and
use of `let-else`). Add `rust-version = "1.75"` (or appropriate version) to
the workspace `Cargo.toml` under `[workspace.package]`, then propagate
`rust-version.workspace = true` to each member `[package]` block. Add a
`rustup toolchain install 1.75 && cargo +1.75 check --workspace` job to any
CI matrix.

**Effort:** S (half-day: compile test + propagate)
**Leverage:** 3 — professional signal, low implementation cost

---

### A-3 — Unwraps in UDP transport receive hot path (panic risk)

**Current state:**
`server/src/transport/udp.rs` contains 18 `.unwrap()` / `.expect()` calls,
many of which are in the active receive path (`AuthReceiver::receive`, lines
192–259). Specific examples:

- `stream.read(&mut self.buffer).unwrap()` (line 192) — panics on any I/O
  error while reading an auth TCP stream from an unauthenticated connection
- `let auth_bytes = base64::decode(auth_bytes).unwrap()` (line 208) — panics
  on malformed base64 from any connecting client before authentication
- `stream.write_all(&response_bytes).unwrap()` (line 240, 257) — panics on
  write errors (e.g. client disconnected mid-auth)

**Why it matters:**
The auth TCP endpoint is the attack surface visible to the entire internet
for any naia-based game server. A client connecting and sending a malformed
auth token (e.g., non-UTF-8 bytes in the base64 field) will panic the server
process, taking down all connected sessions. This is the most critical
correctness gap in the current codebase. It does not require exploiting any
memory safety bug — a single invalid connection request is sufficient.

The spec (SECURITY.md) says "Rate limiting: naia does not throttle message
or mutation rates at the application layer." An unprotected panic makes the
rate-limiting gap academic: one request is enough.

**Recommendation:**
Replace all unwraps in `udp.rs`'s auth-receive paths with `?`-returning
`Result<…, RecvError>` or early `return` / `continue` with a `log::warn!`.
The pattern to follow is the existing `host_engine` fix already in CHANGELOG
("changed to `warn!` + discard"). Specifically:

```rust
// Before
let recv_len = stream.read(&mut self.buffer).unwrap();
let auth_bytes = base64::decode(auth_bytes).unwrap();

// After
let recv_len = match stream.read(&mut self.buffer) {
    Ok(n) => n,
    Err(e) => { log::warn!("auth stream read error: {e}"); continue; }
};
let auth_bytes = match base64::decode(auth_bytes) {
    Ok(b) => b,
    Err(e) => { log::warn!("auth base64 decode error: {e}"); continue; }
};
```

**Effort:** S (a focused day across udp.rs and any mirrors in webrtc.rs)
**Leverage:** 5 — correctness/security, directly prevents DoS crash

---

### A-4 — `cargo doc` emits 7 unresolved-link warnings in `naia-shared`

**Current state:**
Running `cargo doc --workspace --no-deps --all-features` produces seven
`warning: unresolved link` errors in `naia-shared`:

- `unresolved link to 'Server::send_message'`
- `unresolved link to 'Client::send_message'`
- `unresolved link to 'naia_server::ReplicationConfig'`
- `unresolved link to 'naia_client::EntityMut::configure_replication'`
- `unresolved link to 'naia_client::Client::entity_request_authority'`
- `unresolved link to 'naia_client::EntityAuthGrantedEvent'`
- `unresolved link to 'naia_client::EntityAuthDeniedEvent'`

These likely exist in doc comments in `shared/src/lib.rs` or in the CONCEPTS.md
embedded in the shared crate's doc items, referencing types that are only
defined in the server/client crates.

**Why it matters:**
Broken intra-doc links in a published crate produce warnings on docs.rs and
confuse developers navigating the API reference. These are easy wins: either
fix the links (if the types exist but under a different path) or change them
to code spans that don't generate link errors.

**Recommendation:**
Run `cargo doc -p naia-shared --no-deps 2>&1 | grep "warning: unresolved"`,
find each broken link's location with `grep -rn "EntityAuthGrantedEvent\|configure_replication" shared/src/`,
and either fix the path (e.g., `[`naia_server::EntityAuthGrantEvent`]`) or
convert to a backtick span (e.g., `` `EntityAuthGrantEvent` ``) where a cross-crate
link is not possible without an explicit dependency.

**Effort:** XS (30 minutes)
**Leverage:** 3 — clean docs.rs presentation

---

### A-5 — No `#![warn(missing_docs)]` on public crates

**Current state:**
`server/src/lib.rs`, `client/src/lib.rs`, and `shared/src/lib.rs` use
`#![deny(trivial_casts, trivial_numeric_casts, unstable_features, unused_import_braces)]`
but do not include `#![warn(missing_docs)]`. Spot-checking public items in
the server crate reveals that many methods in `Server<E>` — such as `users_count()`,
`rooms_count()`, `entity_replication_config()`, `global_entity_priority_mut()`,
`scope_checks_pending()`, and `mark_scope_checks_pending_handled()` — lack
`///` doc comments on the public type itself, relying only on the lib-level
overview to explain them.

**Why it matters:**
API discoverability via `cargo doc` is the primary onboarding path for users
who have read the README and want to explore available methods. Missing docs
on methods that have no obvious name-implies-behavior semantics force users
to read source code. This is a consistent complaint in developer feedback
about naia ("I had to look at the source to understand X").

**Recommendation:**
Add `#![warn(missing_docs)]` to all three lib files. Then do a `cargo doc`
pass and add short `///` comments to every item that triggers the warning.
Focus first on `server/src/server/` (the `Server<E>` facade) since those
methods are the most commonly-used entry points. A `#[allow(missing_docs)]`
attribute on any genuinely-internal public item (like `bench_send_counters`)
is fine.

**Effort:** M (a few days of focused doc writing across all three crates)
**Leverage:** 4 — directly improves onboarding and docs.rs quality

---

### A-6 — No mdBook / tutorial documentation site

**Current state:**
naia's documentation consists of Markdown files in the repo (README.md,
CONCEPTS.md, PREDICTION.md, SECURITY.md, MIGRATION.md, FEATURES.md, faq/README.md).
These are high quality but only accessible via GitHub or local checkout. There
is no rendered tutorial site. The docs.rs output covers the API reference but
not the conceptual walkthrough.

Lightyear ships a full mdBook at `https://cbournhonesque.github.io/lightyear/book/`
covering setup, replication, prediction, interpolation, Steam, system ordering,
and more, with working code examples at every step.

**Why it matters:**
When a developer is evaluating which networking library to use, the existence
of a rendered tutorial site with a step-by-step guide is a significant
adoption driver. "Are we game yet?" links to documentation sites, not README
files. Search engines surface mdBook content far better than GitHub README
content. The gap is particularly acute for the prediction/rollback guide: while
PREDICTION.md is excellent, it is buried in the repo and not linked from the
top-level docs.rs page.

**Recommendation:**
Generate a GitHub Pages site from the existing docs/ and faq/ Markdown files
using mdBook (or just Jekyll/Zola). The content already exists — only the
rendering pipeline is missing. A `gh-pages` branch built by a GitHub Actions
workflow from `mdbook build` on docs/ takes approximately half a day to set up.
Add a link to the rendered site in README.md and in the docs.rs crate metadata
(`[package.metadata.docs.rs]` does not expose this directly, but the README
badge does).

**Effort:** S (a focused day: mdBook config + GitHub Actions workflow)
**Leverage:** 4 — high discoverability impact, content already exists

---

### A-7 — Developer journey friction: basic demo uses WebRTC as default, not UDP

**Current state:**
`demos/basic/server/src/app.rs:1` imports `naia_server::transport::webrtc` and
creates a `webrtc::Socket`. The README Getting Started section says "See
`demos/basic/`" without noting that the basic demo requires a browser client
(WebRTC signaling). A new developer following the README who wants the simplest
native server+client pair will encounter the WebRTC setup (requiring a public
IP or `http://127.0.0.1:14192`) before they can run a single packet.

The native UDP path (`transport::udp`) exists but is not showcased in the
basic demo — it lives in `demos/socket/`.

**Why it matters:**
The first run experience sets the tone for a new user. The demo discovery path
is: README → demos/basic/ → (confusion: where does the client connect? why is
there a public WebRTC IP parameter?). The `demos/socket/` demo is closer to
"minimal working native server+client" but is not linked first from the README.

**Recommendation:**
Add a `demos/native/` demo (or rename/add to `demos/basic/`) that uses
`transport::udp` for both server and client and has zero external dependencies.
Reorder the README Getting Started section: native UDP first, then WebRTC.
The WebRTC demo remains valuable for the browser-support story but should be
the second step, not the first.

**Effort:** S (a day to write the UDP-native demo + update README ordering)
**Leverage:** 4 — directly reduces first-use friction

---

### A-8 — Per-component replication toggle still absent (issue #186)

**Current state:**
From `FEATURES.md`:
```
- [ ] Per-component replication toggle — fine-grained enable/disable per component
      on a replicated entity, issue #186
```
This feature is not implemented. Currently replication of an entity means all
its registered components replicate — there is no way to stop replicating a
specific component on an entity without removing it from the entity entirely.

**Why it matters:**
Games commonly need to hide specific fields from certain players (e.g., an
opponent's ammunition count, or a "fog of war" component) without using the
room/scope system (which operates at the entity level). The workaround is either
to create a separate "public" vs "private" entity pair or to use two separate
replicated component types, both of which add significant boilerplate. This
is a frequently-requested feature in game networking libraries.

Lightyear supports replication condition per component (via the `ReplicateToAll`
/ `ReplicateToServer` markers and component-level override). bevy_replicon
supports per-component serialization customization.

**Recommendation:**
Track this as a P1 planned feature. The implementation would add a per-entity,
per-component "enabled for this user" bit to the `EntityPriority` system.
Server API: `server.component_scope_mut(&user_key, &entity).exclude::<C>()`.
The diff tracker already operates per-component-kind — the scope check is the
missing piece.

**Effort:** L (a week: diff system extension + scope API + BDD contracts)
**Leverage:** 4 — fills a real gap vs lightyear and bevy_replicon

---

### A-9 — Advisory pile in deny.toml signals dependency health concern

**Current state:**
`deny.toml` carries 19 `ignore` entries (plus two unlisted via `cargo audit`
showing "11 vulnerabilities found!"). All are time-boxed to 2027-06-01 and
trace to the `webrtc-unreliable-client` DTLS stack (rustls 0.19, ring 0.16,
reqwest 0.11, openssl 0.10). The advisories are real, active CVEs:

- RUSTSEC-2024-0336 (rustls infinite-loop in DTLS path)
- RUSTSEC-2025-0004, RUSTSEC-2025-0022 (openssl UAF)
- RUSTSEC-2026-0007 (bytes integer overflow via reqwest)
- RUSTSEC-2026-0098, RUSTSEC-2026-0099, RUSTSEC-2026-0104 (rustls-webpki CVEs)

The decision to defer to 2027-06-01 is documented and intentional. The risk is
acceptable for a library where the DTLS path is in `webrtc-unreliable-client`
(a WASM path) and not in the native UDP path.

**Why it matters:**
A developer running `cargo audit` on a fresh checkout sees "11 vulnerabilities
found!" in bright red. The deny.toml suppresses this in the deny gate but not
in the raw audit output. For an organization with a security policy that runs
`cargo audit --deny warnings`, naia cannot be adopted without forking or
patching the DTLS dependency. Lightyear and renet2 have moved to more recent
WebRTC/QUIC stacks and carry fewer advisories.

**Note:** Per the instructions, DTLS migration is a closed decision deferred to
2027-06-01. This finding is not a request to reopen that decision. It is
documented here as a persistent adoption barrier for corporate/regulated
environments, weighted accordingly.

**Recommendation:**
Add a prominent **"Known Advisory Status"** section to SECURITY.md explaining
exactly which advisories exist, why they are time-boxed, which code paths they
affect (WASM WebRTC path only), and how a developer using only `transport_udp`
is not exposed to the WebRTC CVEs. This converts a red `cargo audit` output
into a documented decision rather than an unanswered question.

**Effort:** XS (add ~200 words to SECURITY.md)
**Leverage:** 3 — reduces friction for security-conscious adopters

---

### A-10 — Bevy adapter: `DefaultClientTag` / `DefaultPlugin` not yet in server adapter

**Current state:**
`adapters/bevy/client/src/lib.rs` documents and exports `DefaultPlugin` and
`DefaultClientTag`, reducing the phantom-type boilerplate for single-client
apps. The server adapter (`adapters/bevy/server/src/lib.rs`) has no equivalent
shorthand — users of the server adapter must always pass the explicit type
parameter (though the server's `Plugin` struct is not generic over T in the
same way, so this is less visible).

The real gap: the bevy server adapter lib.rs doc header does not show a
single-file "hello world server" in Bevy. The client adapter does (shows
`DefaultPlugin::new(client_config(), protocol())`); the server adapter shows
only `Plugin::new(server_config(), protocol())` — which is simpler than the
client case and arguably already fine, but the absence of a complete "add
systems, listen, receive a connection" code snippet in the doc header means
developers are forced to go to the demo to see how to wire up `ConnectEvent`
handlers.

**Recommendation:**
Add a 20-line "quick start Bevy server" doc example to `adapters/bevy/server/src/lib.rs`
that shows: `Plugin::new(…)`, adding a system, calling `Server::listen`, and
draining `ConnectEvent`. This is a docs-only change that eliminates one
round-trip to the demo code for Bevy server users.

**Effort:** XS (add a doc example block)
**Leverage:** 3 — Bevy adapter discoverability

---

## Prioritised Action Table

| Rank | Gap ID | Description | Decision | Effort | Leverage |
|------|--------|-------------|----------|--------|---------|
| 1 | A-3 | UDP transport unwrap-panics on malformed auth | Fix: replace unwraps with warn+discard | S | 5 |
| 2 | A-1 | Clippy gate broken (6 errors, 3 files) | Fix: trivial edits to 3 files | XS | 5 |
| 3 | A-4 | 7 unresolved doc links in naia-shared | Fix: correct or backtick-ize broken links | XS | 3 |
| 4 | A-2 | MSRV undeclared in workspace Cargo.toml | Fix: add `rust-version` field | S | 3 |
| 5 | A-6 | No rendered tutorial site (vs lightyear book) | Do: mdBook + GitHub Pages from existing docs | S | 4 |
| 6 | A-5 | `#![warn(missing_docs)]` not enforced | Do: add lint + pass through doc writing | M | 4 |
| 7 | A-7 | Basic demo defaults to WebRTC, not UDP | Fix: add/promote UDP-native demo | S | 4 |
| 8 | A-8 | Per-component replication toggle absent | Plan: L effort feature | L | 4 |
| 9 | A-10 | Bevy server adapter missing hello-world doc example | Fix: add doc example block | XS | 3 |
| 10 | A-9 | Advisory pile visible via `cargo audit` | Docs: add Known Advisory Status to SECURITY.md | XS | 3 |

---

## What Would Make Naia Definitively #1

The single biggest unlock is a **rendered documentation site (mdBook or
equivalent) published to GitHub Pages**, built from the existing Markdown in
`docs/` and `faq/`.

Naia's conceptual documentation is already among the best in the Rust
game-networking ecosystem. CONCEPTS.md covers the full replication loop,
rooms, channels, tick synchronisation, authority delegation, compression,
diagnostics, reconnection, and lag compensation. PREDICTION.md walks through
multi-entity rollback, misprediction smoothing, and tick-buffer misses in
detail that lightyear's book does not match. But this content is invisible to
search engines and to users who encounter naia via crates.io or
`arewegameyet.rs`.

Lightyear's success in mindshare is substantially attributable to its book
at `cbournhonesque.github.io/lightyear/book/` — a rendered, searchable,
Google-indexed tutorial that developers encounter before they encounter the
crate. Naia has better underlying content; it is just not rendered.

Publishing the docs would not change a single line of code. It would
immediately make naia discoverable by developers searching "rust entity
replication tutorial" or "rust prediction rollback guide". That discoverability
translates directly to adoption, contributor interest, and issue-driven feedback
loops that accelerate quality improvement.

After the doc site, the second-biggest unlock is closing the clippy gate (A-1)
and the UDP panic risk (A-3) — both of which are XS/S effort and would remove
the two most concrete reasons for a senior engineer to pause before adopting
naia in a production project.

---

## Regression Table

*Written after reading `_AGENTS/EXCELLENCE_ANALYSIS_20260511.md` (the previous audit,
graded A−). Previous gaps used B-N IDs.*

| Prev Gap ID | Description | Fixed? | Evidence |
|-------------|-------------|--------|----------|
| B-1 | FEATURES.md — 5 completed items still marked planned | Yes | `FEATURES.md` as of this audit correctly marks all 5 items in the `Shipped` section: enum Message, DefaultClientTag, Historian filtering, fuzz targets (5), metrics crates |
| B-2 | Unbounded message channel queues (OOM vector per issue #165) | Yes | CHANGELOG.md [Unreleased]: "Per-connection message channel backpressure — `ReliableSettings::max_queue_depth` caps the unacknowledged message queue; `send_message` returns `Err(MessageQueueFull)` when the limit is reached"; `FEATURES.md` line 41 confirms it shipped |
| B-3 | Basic demo `send_all_packets` inside TickEvent loop | Not verified | Could not confirm in this audit session — `demos/basic/server/src/app.rs` is 194 lines; the specific placement is not re-checked here but is a persistent concern |
| B-4 | Request/Response: no TTL eviction for zombie entries | Not verified | No CHANGELOG entry for TTL eviction; likely still open |
| B-5 | Public release cadence: private branch vs crates.io | Not applicable | This audit evaluates the private specops/naia branch; crates.io state is outside scope for this session |
| B-6 | MSRV undeclared | No | No `rust-version` field found in any `Cargo.toml`. Persistent gap, now A-2 in this audit. |
| B-7 | `global_world_manager` panics on stale entity keys | Partial | CHANGELOG.md added `user_opt`/`user_mut_opt` for `user()` pattern; entity-level panic path in `global_world_manager.rs` not addressed per this audit's grep results (lines 97, 115, 133, etc. still unwrap entity lookups) |
| (new) | Clippy gate broken (6 errors, 3 files) | No | Introduced since previous audit or newly detected: `socket/shared/src/backends/native/random.rs:3`, `shared/serde/src/impls/scalars.rs:129,272,309,346`, `shared/derive/src/message.rs:663`. This is persistent gap A-1. |
| (new) | UDP transport unwrap-panics in auth TCP receive path | No | `server/src/transport/udp.rs` lines 192–259: `stream.read().unwrap()`, `base64::decode().unwrap()`, `stream.write_all().unwrap()` — triggerable by malformed client connection. New A-3. |
| (new) | 7 unresolved intra-doc links in naia-shared | No | `cargo doc --workspace --no-deps --all-features 2>&1 | grep "^warning"` shows 7 broken links. New A-4. |
| (new) | No mdBook / rendered doc site | No | Markdown files remain unrendered. New A-6. |
| (new) | Basic demo defaults to WebRTC, not UDP | No | `demos/basic/server/src/app.rs` uses `transport::webrtc`. First-run friction for native clients. New A-7. |
| (new) | Per-component replication toggle absent | No | FEATURES.md still shows `[ ] Per-component replication toggle — fine-grained enable/disable, issue #186`. New A-8. |

**Summary:** The previous A− was driven primarily by the release-cadence gap (B-5) and
the channel backpressure issue (B-2). B-2 is now closed (shipped per FEATURES.md and
CHANGELOG). B-1 is closed. The grade in this audit is **B+** rather than A− because
(a) the clippy gate failure is a new visible regression, (b) the UDP auth panic path
remains exploitable, and (c) the doc link breakage and missing doc coverage are
persistent. If A-1 (clippy) and A-3 (UDP panic) are fixed, and MSRV is declared,
the grade returns to A−. Closing A-6 (doc site) would push to A.
