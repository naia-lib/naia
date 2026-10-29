# Naia impact selection

The suite is `naia`, declared in `.clerc/impact.toml`. It inventories the
42 public Cargo workspace members: 131 test-bearing targets share 128
logical target IDs (129 legs at kp4 plus `lint:fmt` and six adopted
2026-10-24 targets). Eight static checks form the mandatory floor. These
are target counts, not passing-test counts.

This is the selector adoption authorized by Kepler 97404, with rung
adoption nydus-kp4-rungs (Kepler 133661/133663): every check lands on
exactly one `[[rung]]` R0-R3 and `impact run` is the native aggregate.
The fleet ladder remains defined in Clerc's `docs/TESTING.md`. Naia
`main` moves only on Connor's word; this adoption lands on `dev`.

## Select and explain

Start with a clean checkout at the actual head being qualified:

```sh
clerc impact verify-map
clerc impact plan --base BASE_SHA --head HEAD_SHA --suite naia
clerc impact explain --plan PRINTED_DIGEST
```

The decision is stored at `target/evidence/impact-PRINTED_DIGEST.json`.
It contains the selected IDs, dependency paths and any fallback triggers.
Use those selected IDs and their declared `[[leg]].argv` as the execution
scope. Submit each authorized command through
`clerc plan-admin gate-leg --root . -- ARGV...`; its native Fabro result
and retained attempt are the execution evidence. Do not replace the
selection with `cargo test --workspace` or a second mapping script.

A source change reaches its package's checks and every dependent
package's checks. The dependency graph conservatively includes normal,
build, dev and target-specific local dependencies. This adoption claims
no `cfg(test)` narrowing. An unknown file or a change to the selector or
build policy selects the entire suite. Documentation-only changes retain
the static floor. Excluded private-sibling crates are outside the public
workspace; changes there conservatively fall back rather than being
declared covered by public targets.

## Physical profiles

Every Cargo test ID has an explicit command in the policy. All commands
retain Cargo defaults, name the target selectors, and use
`--no-fail-fast`. Same-name library and binary targets retain both
selectors; none is discarded by deduplication.

Cargo's `required-features` are included in the command. In particular,
`test:naia-test-harness:quiet_tick_progress` is:

```sh
clerc plan-admin gate-leg --root . -- cargo test -p naia-test-harness --test quiet_tick_progress --features deterministic --no-fail-fast
```

Its two tests drive the real client and server through an in-memory local
link with an injected clock and parked workers. The server library's
command enables `test_time,transport_local,test_utils`, which includes
the three `quiet_data_tests` added with that regression. It retains
active-worker coverage; only the separate quiet-link target enables
`deterministic`. The server port-release targets explicitly enable
`transport_udp`, or both `transport_udp,transport_webrtc`, matching
their crate-level gates. The client UDP floor leg is:

```sh
lint:clippy-client-udp = cargo clippy -p naia-client --all-targets --features transport_udp --keep-going -- -D warnings
```

Do not infer a rung from a Cargo target kind. Rung placement is curated
in `.clerc/impact.toml` (R0 static floor; R1 audited-pure unit lib
targets; R2 in-process integration; R3 the real-port bindings).
The six raw-provider loopback tests relocated from
`webrtc-unreliable-client` (2026-10-29, Kepler ruling ii: the provider
path dev-dep cannot resolve vendored) run as
`test:naia-server-socket:webrtc_unreliable_loopback` on R3, where both
client and server exist.
A metadata target with zero executed tests is never behavioral coverage;
report the native test counts and ignore counts alongside every result.

## Static floor and existing push protection

The floor names file-set formatting, strict workspace Clippy, native
warning-clean checking, the public-path invariant, and all four existing
Wasm profiles. The tracked `tools/hooks/pre-push` continues to enforce
its current native/Wasm checks and its unconditional protection of
`refs/heads/main`; this map does not extend that legacy runner.

Run file-set formatting with
`clerc plan-admin verify-fmt-fileset --root .`. Its checked-file count
is the format denominator; `cargo fmt --all --check` is only the existing
fast pre-check. Strict Clippy uses the policy's workspace/all-targets
command with `-D warnings`. The native and Wasm `cargo check` profiles
retain the hook's `RUSTFLAGS="-D warnings"`; that standard Cargo
environment value must be present when their `gate-leg` captures its
key. The mquad Wasm profile retains its explicit `-D warnings`.

The public-path command examines every workspace member manifest and
refuses a dependency escaping this public repository. Never add private
sibling path dependencies to make a test executable. Cargo.lock remains
an ignored developer artifact, as required by this library repository.

Before a `dev` landing, qualify the committed content and require
`clerc plan-admin land-key --root . --commit HEAD -- cargo clippy --workspace --all-targets --keep-going -- -D warnings`.
A native process success without valid content-key credit is UNKNOWN.
