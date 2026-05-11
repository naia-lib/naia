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

### 1.3 Identify gaps

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
```

Gap IDs should be fresh — do not try to reuse IDs from any previous analysis.
Use a simple scheme: `A-1`, `A-2` … (A for Audit, number for rank order at
time of writing).

---

## 3. Ground rules

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

## 4. After writing the document

1. Commit the file with message:
   `Excellence audit <YYYYMMDD>: fresh gap analysis`
2. Push to `origin dev` via:
   `git push origin main:dev`
3. Report a one-paragraph summary of the top 3 findings to the user.
