# Naia mdBook Review Repair Plan - 2026-05-13

## Goal

Revise the 0.25.0 naia mdBook so it is accurate to the current repository,
Bevy-first where intended, honest in competitor comparisons, and stronger as a
sales document without drifting into claims the project cannot defend.

## Source of truth

- Connor review: `_AGENTS/CONNOR_BOOK_FEEDBACK.md`
- Current code and demos in this repository
- Published workspace crate manifests
- Current competitor docs where comparison claims are made

## Global corrections

- [x] Treat `transport_webrtc` as the preferred production-ready transport for
  both native and Wasm clients from the same native server.
- [x] Present `transport_udp` as plaintext and appropriate for local dev,
  trusted LANs, or advanced users who intentionally add their own security.
- [x] Remove all `transport_quic` references and future-mobile speculation.
- [x] Remove nonexistent public crates: `naia-macroquad-client`,
  `naia-socket-native`, `naia-socket-webrtc`, and `naia-socket-local`.
- [x] Update version examples from `0.24` to `0.25`.
- [x] Use Bevy crates/snippets outside the "Without Bevy" section.
- [x] Use Bevy 0.18 `MessageReader<T>` in reader-facing examples instead of
  lower-level `ResMut<Messages<T>>` or older `EventReader<T>` wording.
- [x] Explain that server-authoritative replication is the default pattern, but
  not the whole model: client-authoritative entities are opt-in via
  `Protocol::enable_client_authoritative_entities()`.
- [x] Clarify delegated authority as separate from client-authoritative
  entities: delegation can migrate published client-owned entities/resources
  into server-owned delegated state and can also be configured on server-owned
  entities/resources.
- [x] Clarify Bevy replication: entities must call `enable_replication()`;
  otherwise they remain local Bevy entities.

## Page checklist

- [x] `SUMMARY.md`: remove Live Demo and Migration Guide; rename WebRTC page to
  reflect native + Wasm support.
- [x] `introduction.md`: remove monopoly/browser claim, runtime internals, and
  low-level entity trait bounds; improve differentiators and quick concepts.
- [x] `getting-started/why-naia.md`: stronger short answer, accurate browser
  claims, remove runtime internals and Tribes 2 aside.
- [x] `getting-started/installation.md`: fix Bevy dependency layout, non-Bevy
  layout, macroquad guidance, browser features, and versions.
- [x] `getting-started/bevy-quickstart.md`: update shared crate to
  `naia-bevy-shared`, versions, and WebRTC-first transport snippets.
- [x] `getting-started/first-server.md`: update transport snippets and versions;
  keep it complementary to Quick Start.
- [x] `getting-started/first-client.md`: update browser/native setup and feature
  flags.
- [x] `concepts/protocol.md`: remove bogus Request/Response derive wording.
- [x] `concepts/replication.md`: fix Mermaid syntax and replicated-resource
  explanation.
- [x] `concepts/messages.md`, `concepts/rooms.md`, `concepts/ticks.md`,
  `concepts/connection.md`: consistency pass for Bevy-first, MessageReader-first,
  and WebRTC-first.
- [x] `authority/server-authority.md`: reframe default authority without
  denying client-owned entities/resources.
- [x] `authority/client-owned.md`: correct `Publicity::Private` semantics.
- [x] `authority/delegation.md`: correct default ownership, remove unproven
  patterns, explain migration/delegated resources, add Bevy replication note.
- [x] `authority/publishing.md`: correct `Publicity` wording.
- [x] `advanced/prediction.md`, `advanced/historian.md`: streamline and correct
  timeline/exclusivity claims.
- [x] `advanced/request-response.md`: use Bevy crates and note `Request` /
  `Response` are marker traits, not public derive macros.
- [x] Transport pages: replace fake crate/API claims with current feature/module
  names and working demos as examples.
- [x] Without Bevy pages: expand macroquad/custom guidance and avoid implying
  async is required.
- [x] Performance pages: consistency pass.
- [x] `demo/live.md`: remove from navigation for now.
- [x] `reference/features.md`: reorganize by feature group, remove planned
  non-features and implementation trivia.
- [x] `reference/security.md`: WebRTC-first security guidance.
- [x] `reference/comparison.md`: compare against lightyear, bevy_replicon, and
  matchbox accurately; remove apples-to-oranges rows where they confuse.
- [x] `adapters/bevy.md`: relevance/readability/accuracy pass.
- [x] `reference/glossary.md`: group terms by theme.
- [x] `reference/migration.md`: remove from navigation for now.
- [x] `reference/api.md`: list all published crates in this workspace.
- [x] `reference/faq.md`: reorganize into real user questions.
- [x] `reference/changelog.md`: include the actual changelog content, not only a
  GitHub pointer.
- [x] CI: add a Mermaid validation gate so syntax errors fail before deploy.

## Verification

- [x] Run targeted `rg` checks for removed claims/features/crates and stale
  Bevy event-reader examples.
- [x] Run `mdbook build book/`.
- [x] Run Mermaid validator locally if dependencies are available.
- [x] Review `git diff` for accidental unrelated changes.
