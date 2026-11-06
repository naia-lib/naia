# Naia public packaging: Namako and Slag dependency closure (nydus-v44)

Inventory date: 2026-11-06. Baseline: naia `dev` at `e6109ee1`
(42 workspace members). Owner note: Nash owns naia and namako; slag is
Brock's private repo.

## Shipping closure

All 42 workspace members were scanned with the
`check_no_escaping_path_deps` algorithm (member-block parse, per-manifest
`path = "..."` scan with comment stripping, symlink-resolved prefix check
against the repo root, plus an absolute-path sweep): **zero repo-escaping
path dependencies, zero missing member manifests.** External consumers
cloning naia with no siblings present get the full library; only internal
harness CLIs stay behind (Connor ruling 2026-09-17, encoded in the root
`Cargo.toml` comment and `_AGENTS/RELEASE_PROCESS.md`). Intra-workspace
edges use the standard `version = "0.25"` + `path` dual form, which
resolves by version for external consumers.

## Crates outside the workspace (all `publish = false`)

| Crate | Escaping edge | Status |
|---|---|---|
| `test/npa` | `namako_engine`, path `../../../namako/engine` | excluded, unpublished |
| `test/tests` | `namako_engine`, path `../../../namako/engine` | excluded, unpublished |
| `test/bevy_npa` | `namako_engine`, path `../../../namako/engine` | excluded, unpublished |
| `bench/suite` | `namako_engine`, path `../../../namako/engine` | excluded, unpublished |
| `bench/wins` (`naia-bench`) | `crucible_core`, path `../../../slag/crates/platform/crucible/core` | excluded, unpublished, non-member |

`bench/wins` stays exactly as it is per Ziph 2026-11-06: no move and no
deletion this card. Relocating naia's win-checks to their private owner is
a what/why call with Trevor; if ruled, it becomes its own card with Brock.

## Published-version map

- `namako_engine` 0.1.0 is **not published**: crates.io answers 404 for
  both `namako` and `namako_engine`. No published substitute exists.
- Slag's `crucible_core` 0.1.0 is private. The `crucible-core` on crates.io
  (newest 0.1.11) is an unrelated third-party crate, not a substitute.

## Provider-release dependencies (explicit, not vendored)

1. `namako_engine` publication — provider: namako (Nash). Recorded, not
   executed: this card forbids release/publication. Publication becomes its
   own card.
2. `crucible_core` publication or relocation — provider: slag (Brock).

Until those land, the four namako test/bench consumers and `bench/wins`
keep their sibling path edges inside the excluded, unpublished set. No
owned public manifest path-depends on private Slag or unpublished Namako.
