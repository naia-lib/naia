# Vendored siblings

Naia is public: external consumers clone it with no siblings present, so no
`path =` dep on this repo may resolve outside the repo root (Connor ruling
2026-09-17, enforced by `lint:public-paths` /
`cargo run -p naia-ci-checks --bin check_no_escaping_path_deps`).

Each directory here is a byte-exact copy of a sibling repo at the SHA pinned
in the root `siblings.lock`, refreshed only by copying again:

    git -C <sibling-checkout> archive <pinned-sha> | tar -x -C vendor
    mv vendor vendor-tmp && mkdir vendor && mv vendor-tmp vendor/<name>

Refresh procedure: check out the sibling at the new pinned SHA, re-archive,
replace the directory whole, re-apply the documented excision below, and
update `siblings.lock` in the same commit. The single permitted delta from
the archived bytes is the excision; never make any other hand-edit. Never
add a vendor dir as a workspace member (they build only as path
dependencies, so `cargo fmt`/`clippy --workspace` do not touch them).

Documented excision (`vendor/webrtc-unreliable-client/Cargo.toml`):
the upstream `[dev-dependencies] naia-server-socket = { path =
"../naia/socket/server" }` block (used only by the sibling's own
`tests/loopback.rs`, never built when consumed as a dependency) is removed
so the vendor copy is self-contained. Fabro's source capture requires every
path in the closure to exist; an out-of-repo back-path would fail capture.

Documented lint patch
(`vendor/webrtc-unreliable-client/src/webrtc/peer_connection/certificate.rs:72`):
`format! {"{:02x}", x}` becomes `format!("{:02x}", x)`. Identical expansion,
zero behavior delta; required because naia's gate lints path dependencies
with `-D warnings` and nightly-2026-08-27's `nonstandard_macro_braces`
fires on the braces form. Filed upstream as a card on the sibling owner;
until the sibling is fixed and re-vendored, re-apply this one-line patch on
refresh.

Removed upstream integration test (`vendor/webrtc-unreliable-client/tests/`
deleted): the only file, `loopback.rs`, drives the sibling's own server
through the excised dev-dep and cannot compile outside the sibling repo.
Integration tests of a dependency are never built for consumers, so removal
changes nothing naia builds; restores automatically on refresh (re-apply the
removal while the dev-dep excision stands).

Contents:
- `webrtc-unreliable-client/` — sibling `webrtc-unreliable-client` at
  `8233c3f8b942631da68adc360ae52a1c4af0329f`. Consumed by
  `socket/client` (native, non-wasm target only). Transport spec
  `test/specs/contracts/02_transport.spec.md` t2 requires these exact sibling
  bytes (never the registry version, which reintroduces a tokio runtime).
