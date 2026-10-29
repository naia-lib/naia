# Vendored siblings

Naia is public: external consumers clone it with no siblings present, so no
`path =` dep on this repo may resolve outside the repo root (Connor ruling
2026-09-17, enforced by `lint:public-paths` /
`cargo run -p naia-ci-checks --bin check_no_escaping_path_deps`).

Each directory here is a byte-exact copy of a sibling repo at the SHA pinned
in the root `siblings.lock`, refreshed only by copying again:

    git -C <sibling-checkout> archive <pinned-sha> | tar -x -C vendor/webrtc-unreliable-client

Refresh procedure: check out the sibling at the new pinned SHA, re-archive,
replace the directory whole, and update `siblings.lock` in the same commit.
The vendor copy must match the archived bytes exactly; never hand-edit.
Never add a vendor dir as a workspace member (they build only as path
dependencies, so `cargo fmt`/`clippy --workspace` do not touch them).

Historical excision (applied at pin `15f7f85` only, retired since `022e9de`):
the upstream `[dev-dependencies] naia-server-socket = { path =
"../naia/socket/server" }` block (used only by the sibling's own
`tests/loopback.rs`, never built when consumed as a dependency) was removed
so the vendor copy was self-contained. Cargo loads the manifests of every
path in the workspace closure, and the back-path resolved to the nonexistent
`vendor/naia/socket/server`, which broke `cargo metadata` for the whole
workspace (demonstrated 2026-10-29: `failed to read
.../vendor/naia/socket/server/Cargo.toml`). Kepler ruling (ii) removed the
dev-dep and its test upstream instead, so no excision applies anymore.

RESOLVED (Kepler ruling (ii), 2026-10-29): the sibling-path consumption
ordered by Usher 133668 / Kepler 133690 tripped the 09-17 invariant above,
as demonstrated by the native `lint:public-paths` leg on restore commit
`21d82b98` (never pushed). Kepler ruled provider-side repair instead: the
unresolvable `../naia/socket/server` dev-dep was removed upstream at
`022e9de` (with its `tests/loopback.rs`, relocated to naia R3 at
`socket/server/tests/webrtc_unreliable_loopback.rs`), so the pinned sibling
is consumed as this vendor copy, fully byte-exact with no excision.

Contents:
- `webrtc-unreliable-client/` — sibling `webrtc-unreliable-client` at
  `022e9dea73a10c1157fe81fab5eae41be3842aec`, byte-exact (provider tree
  `2923b3231ae7b5cfa900b473dc7fa71d43ec8d03`; no excision — upstream carries
  no dev-deps and no `tests/` dir since `022e9de`).
  The three upstream repairs
  (certificate.rs `format!` braces fix, fleet `rust-toolchain.toml` pin,
  `AGENTS.md` spec line) are in the pin, so no re-application. Consumed by
  `socket/client` (native, non-wasm target only). Transport spec
  `test/specs/contracts/02_transport.spec.md` t2 requires these exact sibling
  bytes (never the registry version, which reintroduces a tokio runtime).
