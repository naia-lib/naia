# Excellence Audit — Instructions for the Next Agent

**Purpose:** Produce a fresh, evidence-based gap analysis of naia against the
current state of the art in game networking libraries, and a prioritised
action plan to close those gaps.

Do NOT read any previous audit document before doing your research. Form your
own independent view of the codebase and the competitive landscape first.

---

## 1. What to audit

### 1.1 Read the naia codebase

Start from the workspace root. Understand:

- What naia actually ships today: transport options, replication model, channel
  types, authority delegation, interest management (rooms + user scope),
  priority/bandwidth machinery, tick synchronisation, fragmentation.
- The public API surface: `server/src/lib.rs`, `client/src/lib.rs`,
  `shared/src/lib.rs`, `adapters/bevy/*/src/lib.rs`.
- Documentation: `README.md`, `docs/CONCEPTS.md`, `docs/PREDICTION.md`,
  `SECURITY.md`, `CHANGELOG.md`.
- Test/quality surface: the `namako/` BDD harness, `shared/serde/tests/`,
  `benches/`, the fuzz targets.

Do NOT assume anything about what was true in a previous audit session. Code
changes. Read what is there now.

### 1.2 Survey the competition

Research the current state of these libraries (docs, READMEs, changelogs,
GitHub issues, recent release notes):

- **lightyear** (`https://github.com/cBournhonesque/lightyear`) — Bevy-native,
  client prediction+rollback, WebTransport, interest management, Steam relay.
- **renet / renet2** (`https://github.com/lucaspoffo/renet`) — netcode.io auth,
  simple API, many shipped games.
- **bevy_replicon** (`https://github.com/projectharmonia/bevy_replicon`) —
  clean Bevy replication, replicon-quinnet for QUIC.
- **Quinn** (`https://github.com/quinn-rs/quinn`) — production QUIC in Rust,
  TLS 1.3.
- **GameNetworkingSockets** (Valve) — production at scale, per-lane reliability,
  dev simulation tools.
- Any other libraries that have become prominent since the last audit.

What are developers saying right now? Check:
- `r/rust_gamedev` recent posts mentioning naia or alternatives.
- GitHub issues and discussions on naia itself.
- Any blog posts or tutorials published in the last 12 months.

### 1.3 Run the Rust quality checklist

Before forming any gap opinion, run these commands and record the raw output:

```
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps --all-features 2>&1 | grep "^warning"
cargo audit
grep -rn "\.unwrap()\|\.expect(" server/src/ client/src/ shared/src/ \
  | grep -v "#\[cfg(test)\]" \
  | grep -v "^\s*//"
```

For each command: note whether it is clean, and if not, quote the specific
failures. An `unwrap()` in a connection-handling hot path is a silent
correctness gap — a malformed packet should never crash the game server.
Treat any `clippy` failure under `--all-targets` as a gap. Treat any
`cargo audit` advisory not already listed in `deny.toml` as a gap.

Also check:
- Is MSRV declared in the workspace `Cargo.toml`? Is it tested anywhere?
- Do all public items in `server/src/lib.rs`, `client/src/lib.rs`,
  `shared/src/lib.rs` have `///` doc comments?
- Does `cargo doc` produce any "missing documentation" warnings on public items?

### 1.4 Developer journey test

Starting from only the `README.md` and the `demos/` directory (no prior
knowledge of the codebase), sketch the steps a new user would follow to
build a minimal working server + client that:

1. Establishes a connection
2. Spawns one entity on the server
3. Replicates one component update to the client

Document every friction point: missing doc, wrong assumption, API that
requires reading source to understand. This is distinct from "what is
missing" — it is "what is confusing to a first-time user even if it
technically exists."

### 1.5 Identify gaps

For each gap you find, ask:
- Does naia lack this entirely, or does it exist but undocumented/undiscoverable?
- Does a competitor ship this in a way that gives them a real developer-experience
  advantage?
- What would a developer who hits this gap do — abandon naia, work around it, or
  not notice?

Categories to cover (not exhaustive — add your own if warranted):

| Category | Example questions |
|----------|-------------------|
| Transport | Is plain UDP still the only native option? Is QUIC/WebTransport still absent? Any new security concerns? |
| Encryption | What is the recommended production path for native clients? Is that clearly documented? |
| Developer experience | Is the API discoverable? Are errors ergonomic? Are common patterns in the docs? |
| Prediction / rollback | Is the guide sufficient? What does a developer still have to figure out themselves? |
| Testing / reliability | Are there coverage gaps? Integration gaps? Fuzz targets? |
| Performance | Any benches that regressed or that reveal headroom vs competitors? |
| Documentation | Missing sections, broken examples, outdated code snippets? |
| Ecosystem fit | Bevy adapter quality vs lightyear? ECS-agnostic story still valid? |
| Scalability | Is the single-process story clearly communicated? Any footguns? |
| Community signals | What questions keep coming up on Discord/issues? |

---

## 2. Output format

Write the results to a new file: `_AGENTS/EXCELLENCE_ANALYSIS_<YYYYMMDD>.md`.
Use today's date.

Structure the document as follows:

```
# Naia Excellence Audit — <date>

## Executive Summary
3–5 sentences: what is naia's current standing, what are the top 3 gaps,
what one change would have the most impact.

## Competitive Landscape
One table: competitor, strengths vs naia, weaknesses vs naia.
Update based on what competitors actually ship today.

## Gap Analysis
One subsection per gap found. For each:
- Current state (what naia has or lacks today — cite file:line)
- Why it matters (developer impact, competitive impact)
- Recommendation (concrete action or explicit deferral with rationale)
- Effort: XS / S / M / L / XL
- Leverage: 1–5

## Prioritised Action Table
A single ranked table of all actionable items, ordered by (leverage × urgency).
Columns: Rank | Gap ID | Description | Decision | Effort | Leverage

## What Would Make Naia Definitively #1
Short section: what is the single biggest unlock? Is it a feature, a doc, a
transport change, an ecosystem move?

## Regression Table
After forming your independent analysis, read `_AGENTS/EXCELLENCE_ANALYSIS_20260511.md`
and complete this table. Do NOT read this file before forming your own analysis.

| Prev Gap ID | Description | Fixed? | Evidence |
|-------------|-------------|--------|----------|
| …           | …           | Yes/No/Partial | file:line or commit |

A gap that appeared in the previous audit and is still present is a
**persistent gap** — weight it more heavily in your prioritisation.
A gap that is now absent should show a commit hash or the doc/code location
that resolved it.
```

Gap IDs should be fresh — do not try to reuse IDs from any previous analysis.
Use a simple scheme: `A-1`, `A-2` … (A for Audit, number for rank order at
time of writing).

---

## 3. Out-of-scope — do not re-litigate these decisions

The following are **closed decisions** made by the project owner. Do not
include them as gaps, do not propose revisiting them, and do not spend
audit time on them. If evidence emerges that a decision is actively causing
harm (e.g. a security advisory with a concrete CVE), flag that as a new
finding — but framing it as re-opening the old decision.

### Transport architecture decisions

| Topic | Decision | Rationale |
|-------|----------|-----------|
| **QUIC / `transport_quic`** | Deferred — do not propose | XL effort; existing UDP transport covers the target use cases. No implementation work without explicit instruction. |
| **WebTransport / `transport_webtransport`** | Deferred, blocked on QUIC | WebTransport = HTTP/3 over QUIC; implement QUIC first to share the infra. Urgency is low while WebRTC still works. |
| **DTLS advisory pile in `webrtc-unreliable`** | Deferred to 2027-06-01 | Advisories are time-boxed in `deny.toml`; do not raise before that date. |
| **Single-socket architecture** | Closed — not a gap | WebRTC already serves native + WASM clients from one process. No multi-socket work needed. |
| **TypeScript/JavaScript client** | Deferred indefinitely | XL effort, no peer ships one, Rust→WASM covers the browser use case. |

### Security decisions

| Topic | Decision | Rationale |
|-------|----------|-----------|
| **Auth payload plaintext over UDP** | Resolved by `SECURITY.md` + transport docs | The warning is documented; the fix is transport selection (QUIC when available). No per-method additions needed. |
| **Packet integrity for native UDP** | Docs-only | Random corruption is handled by `SerdeErr` discard; deliberate injection is addressed by transport selection. No code changes. |

### Feature scope decisions

| Topic | Decision | Rationale |
|-------|----------|-----------|
| **Built-in client prediction framework** | Not a gap | `CommandHistory` + `TickBuffered` + `local_duplicate()` + demo code + `docs/PREDICTION.md` are the reference implementation. Naia supplies the primitives; the application assembles the loop. |
| **Built-in snapshot interpolation framework** | Not a gap | The Bevy and Macroquad demos already implement it via an `Interp` component. Same philosophy as prediction. |
| **Spatial / automatic interest management** | Out of scope | Rooms = coarse; `scope_checks_pending()` = fine-grained hook point. A spatial hash belongs in a third-party crate. Same reason naia has no physics. |
| **Horizontal scaling / multi-server** | Deferred indefinitely | Application-architecture concern; all peers leave this to the developer. The zone-sharding pattern is documented in `docs/CONCEPTS.md`. |
| **Packet replay / traffic recording** | Deferred indefinitely | ECS-snapshot replay (via `Historian`) is more useful for desync debugging than raw byte replay. Revisit only with a concrete desync use case. |
| **AIMD congestion control** | Closed — not applicable | AIMD is a TCP-stream concept. State sync congestion response is already correct: defer lower-priority entities, compound their priority. The token bucket handles reliable pacing. |

### Design invariants — do not challenge these

These are load-bearing architectural choices. Do not recommend changing them.

- **ECS-agnostic core.** `Server<E>` and `Client<E>` are generic over the
  entity type. The Bevy adapter is a thin layer on top, not the core. This is
  a deliberate differentiator vs lightyear.
- **Server-authoritative model.** Naia is not a P2P library. Authority
  delegation is bounded (server grants/revokes). Do not propose P2P modes or
  symmetric authority.
- **No physics / no spatial queries in naia core.** The `Historian` provides
  lag-compensation primitives; the application does hit detection. Naia will
  not bundle a physics engine.
- **naia-as-async-std/smol.** The async runtime is smol/async-std, not tokio.
  Do not propose switching runtimes.

---

## 4. Grading rubric

Assign naia an overall letter grade. Use this rubric — do not invent your
own scale. The grade must appear in the Executive Summary.

| Grade | Meaning |
|-------|---------|
| **S** | Industry-leading — would be recommended over lightyear / renet for most new projects without reservation |
| **A** | Production-ready — no significant gaps vs peers; minor papercuts only |
| **B** | Usable — meaningful gaps that noticeably affect adoption or developer experience |
| **C** | Functional — missing table-stakes features or has correctness concerns that would give a senior engineer pause |
| **D** | Significant reliability or API problems that make production use risky |

Apply `+` / `-` within a grade where evidence warrants it (e.g. `B+`).
State the single thing that, if fixed, would move the grade up one tier.

## 5. Ground rules

- **Evidence first.** Every gap claim must be grounded in something you
  actually read (a file, a competitor page, a GitHub issue). No guessing.
- **Read before proposing.** Before recommending a library-level change,
  read the relevant source so you know what actually exists.
- **Cite locations.** For gaps in naia code or docs, include `file:line`.
  For competitor gaps, include a URL or source.
- **Be specific.** "Improve docs" is not actionable. "Add a `##
  Reconnection` section to `CONCEPTS.md` showing the `disconnect` +
  `connect` sequence" is.
- **Calibrate effort honestly.** XS = one file, one hour. S = a day. M = a
  few days. L = a week+. XL = multiple weeks or external dependencies.
- **Do not anchor on past findings.** The value of a fresh audit is an
  independent view. Do not read `_AGENTS/EXCELLENCE_ANALYSIS.md` (the
  previous audit) before forming your assessment. After you have written
  your own findings, you may compare against the old doc to check for
  regressions or missed items — but your primary analysis must be
  independent.

---

## 6. After writing the document

1. Read `_AGENTS/EXCELLENCE_ANALYSIS_20260511.md` and complete the
   Regression Table in your new document. This step is mandatory — do not
   skip it.
2. Commit the file with message:
   `Excellence audit <YYYYMMDD>: fresh gap analysis`
3. Push to `origin dev`.
4. Report to the user:
   - The overall grade (and what changed vs the previous `B-` if relevant)
   - The top 3 new or persistent gaps
   - The single change that would move the grade up one tier
