# Cargo Publish Plan — v0.25.0

Systematic, step-by-step plan for merging `dev` → `main`, bumping all crate versions,
and publishing to crates.io. Follow in order. Do not skip steps.

---

## Pre-flight checks

Before touching anything:

```sh
# confirm you are on dev and it is clean
git status
git log --oneline -5

# full workspace compile — must be green
cargo check --workspace

# wasm32 gate
cargo run -p automation_cli -- check-wasm
```

If any check fails, fix it before proceeding.

---

## Phase 1 — Version bump

All publishable crates move from `0.24.x` → `0.25.0`.
The two non-publishable crates (`naia-metrics`, `naia-bevy-metrics`) already carry
`0.25.0` and `publish = false`; leave them alone.

### 1A — Bump `version =` in every publishable `Cargo.toml`

Edit each file listed below. Change `version = "0.24.x"` to `version = "0.25.0"`.

| File | Old version | New version |
|------|-------------|-------------|
| `shared/serde/derive/Cargo.toml` | `0.24.0` | `0.25.0` |
| `shared/serde/Cargo.toml` | `0.24.0` | `0.25.0` |
| `shared/derive/Cargo.toml` | `0.24.0` | `0.25.0` |
| `shared/Cargo.toml` | `0.24.0` | `0.25.0` |
| `socket/shared/Cargo.toml` | `0.24.0` | `0.25.0` |
| `socket/client/Cargo.toml` | `0.24.1` | `0.25.0` |
| `socket/server/Cargo.toml` | `0.24.0` | `0.25.0` |
| `client/Cargo.toml` | `0.24.0` | `0.25.0` |
| `server/Cargo.toml` | `0.24.0` | `0.25.0` |
| `adapters/bevy/shared/Cargo.toml` | `0.24.0` | `0.25.0` |
| `adapters/bevy/client/Cargo.toml` | `0.24.0` | `0.25.0` |
| `adapters/bevy/server/Cargo.toml` | `0.24.0` | `0.25.0` |
| `metrics/Cargo.toml` | already `0.25.0` | no change |
| `adapters/bevy/metrics/Cargo.toml` | already `0.25.0` | no change |

### 1B — Update intra-crate dep version strings

All `[dependencies]` entries that reference naia crates by path already carry
a `version = "0.24"` alongside the `path`. Update each to `"0.25"`.

Files to touch and the specific dep lines to change:

**`shared/serde/Cargo.toml`** — `[dependencies]`
```toml
naia-serde-derive = { version = "0.25", path = "derive" }
```

**`shared/derive/Cargo.toml`** — `[dependencies]`
```toml
naia-serde-derive = { version = "0.25", path = "../serde/derive" }
```

**`shared/Cargo.toml`** — `[dependencies]`
```toml
naia-socket-shared = { version = "0.25", path = "../socket/shared" }
naia-derive        = { version = "0.25", path = "derive" }
naia-serde         = { version = "0.25", path = "serde" }
```

**`socket/client/Cargo.toml`** — `[dependencies]`
```toml
naia-socket-shared = { version = "0.25", path = "../shared" }
```

**`socket/server/Cargo.toml`** — `[dependencies]`
```toml
naia-socket-shared = { version = "0.25", path = "../shared" }
```

**`client/Cargo.toml`** — `[dependencies]`
```toml
naia-shared        = { version = "0.25", path = "../shared" }
naia-client-socket = { version = "0.25", path = "../socket/client", optional = true }
```

**`server/Cargo.toml`** — `[dependencies]`
```toml
naia-shared        = { version = "0.25", path = "../shared" }
naia-server-socket = { version = "0.25", path = "../socket/server", optional = true }
```

**`adapters/bevy/shared/Cargo.toml`** — `[dependencies]`
```toml
naia-shared = { version = "0.25", path = "../../../shared", features = ["bevy_support", "wbindgen"] }
```

**`adapters/bevy/client/Cargo.toml`** — `[dependencies]`
```toml
naia-client      = { version = "0.25", path = "../../../client", features = ["bevy_support", "wbindgen"] }
naia-bevy-shared = { version = "0.25", path = "../shared" }
```

**`adapters/bevy/server/Cargo.toml`** — `[dependencies]`
```toml
naia-server      = { version = "0.25", path = "../../../server", features = ["bevy_support"] }
naia-bevy-shared = { version = "0.25", path = "../shared" }
```

Note: the `[dev-dependencies]` block in `adapters/bevy/server/Cargo.toml` uses
path-only entries — that is correct and fine for dev-deps (`cargo publish` does not
require a version field there).

**`metrics/Cargo.toml`** — `[dependencies]` and `[target.'cfg(target_arch = "wasm32")'.dependencies]`
```toml
naia-shared = { version = "0.25", path = "../shared", features = ["observability"] }
```
(both occurrences; the wasm32 target block also adds `"wbindgen"` to features — leave that unchanged)

**`adapters/bevy/metrics/Cargo.toml`** — `[dependencies]`
```toml
naia-bevy-shared = { version = "0.25", path = "../shared" }
naia-bevy-server = { version = "0.25", path = "../server", optional = true }
naia-bevy-client = { version = "0.25", path = "../client", optional = true }
```
Note: `naia-metrics = { version = "0.25", ... }` is already correct — leave it as-is.

### 1C — Verify after edits

```sh
# regenerate the lock file (no compile needed yet, just dep resolution)
cargo generate-lockfile

# full compile must still be green
cargo check --workspace

# wasm32 check
cargo run -p automation_cli -- check-wasm
```

If anything fails here, fix before proceeding.

---

## Phase 2 — Commit and merge to main

```sh
# stage all Cargo.toml changes
git add \
  shared/serde/derive/Cargo.toml \
  shared/serde/Cargo.toml \
  shared/derive/Cargo.toml \
  shared/Cargo.toml \
  socket/shared/Cargo.toml \
  socket/client/Cargo.toml \
  socket/server/Cargo.toml \
  client/Cargo.toml \
  server/Cargo.toml \
  adapters/bevy/shared/Cargo.toml \
  adapters/bevy/client/Cargo.toml \
  adapters/bevy/server/Cargo.toml \
  metrics/Cargo.toml \
  adapters/bevy/metrics/Cargo.toml \
  Cargo.lock

git commit -m "chore: bump all publishable crates to v0.25.0"

# merge dev into main (fast-forward preferred)
git checkout main
git merge dev --ff-only

# push both branches
git push origin main
git push origin dev
```

If `--ff-only` fails (main has diverged), use `git merge dev --no-ff` and resolve
any conflicts before pushing.

---

## Phase 3 — Dry-run publish

Before touching crates.io, verify every crate passes `cargo publish --dry-run`.
Run in the topological order below (each crate must be published before its dependents).

```sh
cd /path/to/naia   # repo root

cargo publish --dry-run -p naia-serde-derive
cargo publish --dry-run -p naia-serde
cargo publish --dry-run -p naia-socket-shared
cargo publish --dry-run -p naia-derive
cargo publish --dry-run -p naia-shared
cargo publish --dry-run -p naia-client-socket
cargo publish --dry-run -p naia-server-socket
cargo publish --dry-run -p naia-client
cargo publish --dry-run -p naia-server
cargo publish --dry-run -p naia-bevy-shared
cargo publish --dry-run -p naia-bevy-client
cargo publish --dry-run -p naia-bevy-server
cargo publish --dry-run -p naia-metrics
cargo publish --dry-run -p naia-bevy-metrics
```

Fix any errors (missing metadata, bad path refs, etc.) before proceeding to live publish.

---

## Phase 4 — Live publish

Publish in the same topological order. After each publish, wait ~30 seconds before the
next — crates.io index propagation takes a moment and the next crate's dep resolution
needs to see the just-published version.

```sh
cargo publish -p naia-serde-derive
sleep 30

cargo publish -p naia-serde
sleep 30

cargo publish -p naia-socket-shared
sleep 30

cargo publish -p naia-derive
sleep 30

cargo publish -p naia-shared
sleep 30

cargo publish -p naia-client-socket
sleep 30

cargo publish -p naia-server-socket
sleep 30

cargo publish -p naia-client
sleep 30

cargo publish -p naia-server
sleep 30

cargo publish -p naia-bevy-shared
sleep 30

cargo publish -p naia-bevy-client
sleep 30

cargo publish -p naia-bevy-server
sleep 30

cargo publish -p naia-metrics
sleep 30

cargo publish -p naia-bevy-metrics
```

After all 14 publishes succeed, verify on crates.io:
- `https://crates.io/crates/naia-serde-derive/0.25.0` — should exist
- `https://crates.io/crates/naia-bevy-server/0.25.0` — should exist
- `https://crates.io/crates/naia-bevy-metrics/0.25.0` — should exist

---

## Phase 5 — Tag the release

```sh
# create an annotated tag on main
git tag -a v0.25.0 -m "Release v0.25.0"
git push origin v0.25.0
```

Then create the GitHub release:

```sh
gh release create v0.25.0 \
  --title "v0.25.0" \
  --notes "$(cat <<'EOF'
## v0.25.0

### Highlights

- Complete mdBook documentation site (The Naia Book)
- SDD migration: 215 contracts ported to namako BDD framework
- Perf phases 8–10: halo_btb_16v16 benchmark suite, protocol-optimised kind tags (−14.3% wire), hexagonal capacity report
- Priority accumulator (Fiedler pacing layer): full A+B+C
- naia-bevy-metrics crate (observability emission layer)
- Typed-ID sweep: LoginToken, ServiceName, NetworkName
- Desync detection recovery (F-17, F-22, F-38)

### Crates updated

naia-serde-derive, naia-serde, naia-socket-shared, naia-derive, naia-shared,
naia-client-socket, naia-server-socket, naia-client, naia-server,
naia-bevy-shared, naia-bevy-client, naia-bevy-server,
naia-metrics, naia-bevy-metrics
EOF
)"
```

Adjust the release notes body to reflect actual changes for this release.

---

## Checklist summary

- [ ] `cargo check --workspace` green on `dev`
- [ ] `cargo run -p automation_cli -- check-wasm` green
- [ ] All 12 `Cargo.toml` `version` fields bumped to `0.25.0`
- [ ] All intra-crate dep version strings updated to `"0.25"`
- [ ] `cargo generate-lockfile` + `cargo check --workspace` green after edits
- [ ] Commit bumps on `dev`
- [ ] `dev` merged to `main` and both branches pushed
- [ ] All 14 dry-run publishes pass
- [ ] All 14 live publishes succeed (with 30s delays)
- [ ] crates.io pages verified for both ends of the dep chain
- [ ] Annotated git tag `v0.25.0` pushed
- [ ] GitHub release created
