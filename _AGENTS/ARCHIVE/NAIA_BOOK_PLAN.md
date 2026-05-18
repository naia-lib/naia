# Naia mdBook — Campaign Plan

## Goal

Publish the best documentation site in the Rust multiplayer networking ecosystem.
Specifically: beat lightyear's book at `cbournhonesque.github.io/lightyear/book/`
on every axis — depth, structure, visual clarity, and discoverability — while
covering the capabilities lightyear cannot (ECS-agnostic adapters, lag compensation,
priority-weighted bandwidth, smol runtime, embedded live demo).

---

## Competitive Gap Analysis

### What lightyear's book does well
- Progressive structure: Tutorial → Concepts → Guides
- Covers transport, connection, reliability, replication, Bevy integration,
  authority, bandwidth optimization, interest management in dedicated chapters
- Rendered, searchable, Google-indexed

### What lightyear's book is missing (where we beat them)
- **No architecture diagrams** — everything is prose; no packet-flow or replication-lifecycle visuals
- **No troubleshooting / debugging chapter** — silent failures are brutal in game networking
- **No performance tuning chapter** — no guidance on bandwidth budgets, compression, or profiling
- **No glossary** — newcomers have to look up RTT, replication, rollback, etc. externally
- **No non-Bevy integration path** — lightyear is Bevy-only, so they can't document ECS-agnostic adapters
- **No embedded live demo** — can't show the library running in the browser without leaving the docs
- **No honest "should I use this vs X?" decision guide** — developers have to read three READMEs to decide

### Our unique content advantages
- **CONCEPTS.md** (901 lines) — already the most thorough replication mental-model doc in the ecosystem
- **PREDICTION.md** (492 lines) — rollback/lag-compensation depth that lightyear's book doesn't match
- **Historian** — our unique lag-compensation primitive that lightyear lacks entirely
- **Priority-weighted bandwidth allocation** — no competitor documents this; naia is ahead
- **ECS-agnostic** — Bevy + macroquad + custom adapter chapters are exclusive content
- **WebRTC in the browser** — we can embed a *live playable demo* directly in the book
  (the wasm_bindgen demo in `demos/basic/client/wasm_bindgen/` already compiles and runs)

---

## Technology Stack

### mdBook + Preprocessors

| Tool | Purpose | Install |
|------|---------|---------|
| `mdbook` | Core renderer | `cargo install mdbook` |
| `mdbook-admonish` | Material-style callout blocks (NOTE / TIP / WARNING / DANGER) | `cargo install mdbook-admonish` |
| `mdbook-mermaid` | Mermaid.js diagrams inline in Markdown | `cargo install mdbook-mermaid` |
| `mdbook-pagetoc` | Per-page table of contents sidebar | `cargo install mdbook-pagetoc` |
| `mdbook-linkcheck` | CI: validate all internal + external links | `cargo install mdbook-linkcheck` |

### Hosting: GitHub Pages via GitHub Actions

The public naia repo (`naia-lib/naia`) gets a GitHub Pages site at
`naia-lib.github.io/naia` (or a custom domain if desired). Deploy is triggered
on every push to `main` that touches `book/`.

**Deployment workflow** (`book/.github/workflows/deploy-book.yml` committed at
repo root as `.github/workflows/deploy-book.yml`):

```yaml
name: Deploy book to GitHub Pages
on:
  push:
    branches: [main]
    paths: ['book/**']
  workflow_dispatch:
permissions:
  contents: read
  pages: write
  id-token: write
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4
      - uses: peaceiris/actions-mdbook@v2
        with:
          mdbook-version: 'latest'
      - run: |
          cargo install mdbook-admonish mdbook-mermaid mdbook-pagetoc mdbook-linkcheck
          mdbook-admonish install --css-dir book/theme
          mdbook-mermaid install book/
          mdbook build book/
      - uses: actions/upload-pages-artifact@v3
        with:
          path: book/book
      - uses: actions/deploy-pages@v4
        id: deployment
```

### Directory layout inside the repo

```
book/
  book.toml          ← mdBook config (title, authors, preprocessors, output)
  src/
    SUMMARY.md       ← Master table of contents (drives ALL navigation)
    introduction.md
    getting-started/
      installation.md
      ...
    concepts/
      ...
    (etc.)
  theme/             ← Custom CSS overrides, admonish stylesheet
  preprocessors/     ← Any custom preprocessors if needed
```

### book.toml template

Use this exact starting configuration. Running `mdbook-admonish install book/`
and `mdbook-mermaid install book/` after `cargo install` will add the CSS/JS
files and may append additional lines — let them.

```toml
[book]
title = "The Naia Book"
authors = ["naia contributors"]
description = "The definitive guide to the naia game networking library"
language = "en"
src = "src"

[build]
build-dir = "book"

[preprocessor.admonish]
command = "mdbook-admonish"
assets_version = "3.0.2"   # updated by `mdbook-admonish install`

[preprocessor.mermaid]
command = "mdbook-mermaid"

[preprocessor.pagetoc]

[output.html]
additional-js = ["mermaid.min.js", "mermaid-init.js"]   # injected by mdbook-mermaid install
git-repository-url = "https://github.com/naia-lib/naia"
edit-url-template = "https://github.com/naia-lib/naia/edit/main/book/src/{path}"

[output.linkcheck]
# mdbook-linkcheck: only run in CI (slow); skip with MDBOOK_LINKCHECK=false locally
```

**Local scaffold sequence** (run once to initialize):

```sh
cargo install mdbook mdbook-admonish mdbook-mermaid mdbook-pagetoc mdbook-linkcheck
mdbook-admonish install book/     # adds CSS to book/theme/ and updates book.toml
mdbook-mermaid install book/      # copies mermaid.min.js + mermaid-init.js into book/
mdbook build book/                # verify it compiles
mdbook serve book/                # preview at http://localhost:3000
```

### Branching policy

All commits go to the `dev` branch. **Never commit directly to `main`** — main
is touched only at release tag time. The GitHub Actions deploy workflow fires
on pushes to `main` (the public naia-lib/naia repo uses `main` as its trunk);
that is correct for the CI side. For local development in specops/naia, commit
and push to `dev` as normal.

---

## Book Structure (SUMMARY.md)

```markdown
# Summary

[Introduction](introduction.md)

# Getting Started

- [Why naia?](getting-started/why-naia.md)
- [Installation](getting-started/installation.md)
- [Your First Server](getting-started/first-server.md)
- [Your First Client](getting-started/first-client.md)
- [Running the Demos](getting-started/demos.md)

# Core Concepts

- [The Shared Protocol](concepts/protocol.md)
- [Entity Replication](concepts/replication.md)
- [Messages & Channels](concepts/messages.md)
- [Rooms & Scoping](concepts/rooms.md)
- [Tick Synchronization](concepts/ticks.md)
- [Connection Lifecycle](concepts/connection.md)

# Authority & Ownership

- [Server Authority Model](authority/server-authority.md)
- [Client-Owned Entities](authority/client-owned.md)
- [Authority Delegation](authority/delegation.md)
- [Entity Publishing](authority/publishing.md)

# Advanced Features

- [Client-Side Prediction & Rollback](advanced/prediction.md)
- [Lag Compensation with Historian](advanced/historian.md)
- [Priority-Weighted Bandwidth](advanced/bandwidth.md)
- [Delta Compression](advanced/delta-compression.md)
- [zstd Compression & Dictionary Training](advanced/compression.md)
- [Request / Response](advanced/request-response.md)

# Transports

- [Overview](transports/overview.md)
- [Native UDP](transports/udp.md)
- [WebRTC (Browser Clients)](transports/webrtc.md)
- [Local (In-Process)](transports/local.md)
- [Writing a Custom Transport](transports/custom.md)

# ECS Adapters

- [Overview & Adapter Contract](adapters/overview.md)
- [Bevy](adapters/bevy.md)
- [Macroquad](adapters/macroquad.md)
- [Writing Your Own Adapter](adapters/custom.md)

# Performance & Diagnostics

- [Bandwidth Budget Analysis](perf/bandwidth.md)
- [Connection Diagnostics](perf/diagnostics.md)
- [Benchmarking](perf/benchmarks.md)
- [Scaling Considerations](perf/scaling.md)

# 🎮 Live Demo

- [Try It In Your Browser](demo/live.md)

# Reference

- [Feature Matrix](reference/features.md)
- [Security & Trust Model](reference/security.md)
- [Comparing naia to Alternatives](reference/comparison.md)
- [Glossary](reference/glossary.md)
- [Migration Guide](reference/migration.md)
- [API Docs (docs.rs)](reference/api.md)
- [FAQ](reference/faq.md)
- [Changelog](reference/changelog.md)
```

---

## Chapter Breakdown & Source Mapping

### Chapters with existing content (light editing needed)

| Chapter | Source | Estimated work |
|---------|--------|----------------|
| Introduction | `README.md` (top section, before feature list) | XS — extract intro prose |
| Feature Matrix | `FEATURES.md` | XS — reformat as table |
| The Shared Protocol | `docs/CONCEPTS.md` §1 "The Shared Protocol" + §2 "Entities and Components" | S — split + add diagrams |
| Entity Replication | `docs/CONCEPTS.md` §3 "The Replication Loop" + §6 "Static vs Dynamic Entities" + §7 "Replicated Resources" | S — split + add diagrams |
| Messages & Channels | `docs/CONCEPTS.md` §5 "Channels" (all subsections incl. TickBuffered) | S — split + add diagrams |
| Rooms & Scoping | `docs/CONCEPTS.md` §4 "Rooms and Scope" (Room membership + UserScope) | S — split + add diagrams |
| Tick Synchronization | `docs/CONCEPTS.md` §9 "Tick Synchronisation" (Server ticks + Client ticks + Prediction and rollback intro) | S — split + add diagrams |
| Connection Lifecycle | `docs/CONCEPTS.md` §11 "Transport and Wasm" + §12 "Network Condition Simulation" + §19 "Reconnection" | S — combine + add diagram |
| Priority-Weighted Bandwidth | `docs/CONCEPTS.md` §16 "Per-Entity Priority and Bandwidth" + §18 "Diagnostics and Bandwidth Tuning" | S — combine + add diagram |
| Delta Compression | `docs/CONCEPTS.md` §13 "Bandwidth-Optimized Properties" | S — expand + add diagram |
| zstd Compression | `docs/CONCEPTS.md` §17 "Compression" | XS — mostly ready |
| Lag Compensation / Historian | `docs/CONCEPTS.md` §20 "Lag Compensation (Historian)" (all subsections, 120 lines) | S — add diagrams |
| Authority Delegation (concepts) | `docs/CONCEPTS.md` §8 "Authority Delegation" (state machine + trust model + example) | S — add state machine diagram |
| Prediction & Rollback (deep dive) | `docs/PREDICTION.md` (full 492 lines) | S — mostly book-ready |
| Security & Trust Model | `docs/SECURITY.md` | XS — reformat |
| Migration Guide | `docs/MIGRATION.md` | XS — already migration format |
| FAQ | `faq/README.md` | XS — already Q&A format |

### Chapters that need original writing

| Chapter | What it needs | Effort |
|---------|--------------|--------|
| Why naia? | Honest comparison table: naia vs lightyear vs renet vs bevy_replicon vs GGRS | S |
| Installation | `Cargo.toml` snippets for all crates, feature flags, wasm target setup | XS |
| Your First Server | End-to-end UDP server walkthrough from zero | S |
| Your First Client | End-to-end UDP client, connect + receive entity | S |
| Running the Demos | Screenshot, `cargo run` commands for each demo | XS |
| Connection Lifecycle | Handshake → tick loop → disconnect state machine (diagram) | S |
| Authority Delegation | Deep dive on the request/grant/release/revoke state machine (diagram) | S |
| Lag Compensation / Historian | How to snapshot world state, query past state (naia-exclusive) | M |
| Priority-Weighted Bandwidth | The per-entity priority accumulator, how to tune it | S |
| Delta Compression | Per-field diff mask, property mutators | S |
| zstd & Dictionary Training | How to enable, how to train a dict on traffic samples | S |
| Request/Response | Pattern, TTL, disconnect cleanup | XS |
| Transports overview | When to use UDP vs WebRTC vs Local | XS |
| Native UDP transport | Auth TCP, connection flow, config | S |
| WebRTC transport | signaling server, browser client setup, CORS | S |
| Local transport | Use cases: unit tests, AI bots | XS |
| Custom transport | The `Transport` trait contract | S |
| Adapters overview | What an adapter must implement, why ECS-agnostic matters | S |
| Bevy adapter | Plugin setup, system ordering, events, Bevy-specific gotchas | M |
| Macroquad adapter | Minimal game loop integration | S |
| Custom adapter | Step-by-step: implement `WorldMutType`, `WorldRefType` | M |
| Bandwidth Budget Analysis | Real numbers from the bench report, how to read the output | S |
| Connection Diagnostics | RTT percentiles, packet loss metrics, the diagnostics API | S |
| Benchmarking | criterion + iai-callgrind setup, what to measure | S |
| Scaling Considerations | How many CCU per server, recommendations | S |
| **Live Demo** | Embed the wasm_bindgen demo via `<iframe>` or inline WASM in the book page | M |
| Comparison guide | Lightyear / renet / bevy_replicon / GGRS — honest trade-offs | S |
| Glossary | ~40 terms: RTT, replication, rollback, tick, delta-compression, etc. | S |

---

## The Live Demo Page (Killer Differentiator)

Naia is unique in the Rust networking space in having a WebRTC transport that runs
in the browser. The `demos/basic/client/wasm_bindgen/` and
`demos/socket/client/wasm_bindgen/` targets already compile to WASM.

**The plan:** Host the compiled WASM demo artifacts alongside the book on GitHub
Pages, and embed them in the "Try It In Your Browser" chapter via an `<iframe>`.
Developers reading the docs can *play* naia without leaving the page. No other
Rust game networking library can do this.

**Implementation steps:**
1. Add a book CI job that also builds `demos/basic/client/wasm_bindgen` and
   copies the WASM + JS glue into `book/book/demo/` before GitHub Pages deploy
2. Serve a minimal `index.html` that loads the WASM module and connects to a
   lightweight demo server (or a static in-browser simulation using `local` transport)
3. Embed in `demo/live.md` with a raw HTML `<iframe>` block (mdBook passes raw
   HTML through)

If running a live demo server is too operationally heavy, use the `local`
transport to run both client and server in the same WASM binary so the demo
works with zero backend infrastructure.

---

## Mermaid Diagrams to Create

Every "hard to visualize" concept gets a diagram. These live inline in the
relevant chapter as fenced ` ```mermaid ``` ` blocks.

| Diagram | Chapter |
|---------|---------|
| Packet send/receive loop (server tick → connection update → send_all_packets) | Connection Lifecycle |
| Replication state machine (Spawn → Update → Despawn) | Entity Replication |
| Channel reliability modes (unreliable / reliable / tick-buffered) | Messages & Channels |
| Authority delegation state machine (Available → Requested → Granted → Released) | Authority Delegation |
| Prediction + rollback timeline (client tick ahead, server correction, rollback) | Prediction |
| Historian snapshot timeline (how past state is queried for lag compensation) | Historian |
| Priority accumulator per-entity (how bandwidth is allocated across entities) | Bandwidth |
| WebRTC signaling + UDP data flow | WebRTC transport |
| Adapter trait hierarchy (WorldMutType / WorldRefType / etc.) | Custom Adapter |

---

## Admonish Callout Conventions

Establish consistent callout semantics across all chapters:

```
```admonish note
Background information a reader might want but can skip.
```

```admonish tip
A shortcut or non-obvious pattern worth knowing.
```

```admonish warning
A gotcha that silently produces wrong behavior.
```

```admonish danger
A panic or security risk if this step is skipped.
```
```

Every chapter should have at least one callout. The "danger" callout in particular
should flag real footguns (e.g., "calling `send_all_packets` inside the
TickEvent loop adds a full tick of latency").

---

## Phase Plan

### Recommended session scope

A single coding session should target **Phase 1 + Phase 2** (scaffold + content
migration). Phase 3 (original writing) and Phase 4 (live demo) are separate
sessions. Do not start Phase 3 until Phase 2 is fully committed and `mdbook build`
passes with zero errors on all migrated chapters.

### Phase 1 — Scaffold (XS)
- Create `book/` directory structure with the exact `book.toml` from the template above
- Run `mdbook-admonish install book/` and `mdbook-mermaid install book/` to inject CSS/JS
- Write the full `SUMMARY.md` with every chapter path (all chapters as stubs containing just a `# Title` line)
- Write the GitHub Actions deploy workflow at `.github/workflows/deploy-book.yml`
- Verify `mdbook build book/` passes locally with zero errors
- Commit + push to `dev`

**Manual step for Connor (cannot be done by agent):** After the commit is pushed
and merged to `main` on `naia-lib/naia`, go to the repo **Settings → Pages → Source**
and select **"GitHub Actions"**. The deploy workflow will then fire on the next `main` push.

**Done when:** `mdbook build book/` succeeds locally; all chapters render as stubs.

### Phase 2 — Content migration (S per chapter)
- Port existing docs into the book structure (CONCEPTS.md split → ~6 chapters, PREDICTION.md, SECURITY.md, MIGRATION.md, FAQ, FEATURES.md)
- Add admonish callouts and correct internal links
- Add mermaid diagrams for each migrated chapter

**Done when:** All "existing content" chapters are populated and linkcheck passes.

### Phase 3 — Original chapters (M total)
- Write the 9 chapters that need original content: Why naia?, First Server/Client walkthrough, Authority Delegation deep dive, Historian, Bandwidth, Transports, Adapters, Comparison guide, Glossary
- Each chapter has runnable code snippets and at least one mermaid diagram

**Done when:** SUMMARY.md has 0 stub chapters.

### Phase 4 — Live demo (M)
- Build wasm_bindgen demo in CI, copy artifacts into book output
- Write `demo/live.md` with embedded `<iframe>`
- Validate demo runs in Firefox + Chrome

**Done when:** Opening the live demo chapter plays a real naia session in the browser.

### Phase 5 — Polish (S)
- Custom CSS theme (naia brand colors if any)
- mdbook-linkcheck in CI (fail build on broken links)
- Add book link to README badge row and docs.rs metadata
- Submit to arewegameyet.rs

**Done when:** CI is green, linkcheck passes, README links to the book.

---

## Success Metrics

When this campaign is complete, someone Googling "rust entity replication tutorial"
or "rust game networking prediction rollback" should land on the naia book before
any other result. Concrete checks:

- [ ] Every chapter in SUMMARY.md is non-stub
- [ ] `mdbook-linkcheck` passes with 0 errors in CI
- [ ] The live demo runs in Chrome and Firefox without a backend server
- [ ] The "Why naia?" comparison page is accurate and honest
- [ ] Google indexes the site (submit via Search Console)
- [ ] arewegameyet.rs lists the book URL
- [ ] README badge row links to the book
