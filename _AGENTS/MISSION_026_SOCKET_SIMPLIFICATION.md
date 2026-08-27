# MISSION: naia 0.26 — socket trait-box removal, IdentityToken newtype, handshaker unification

Status: IN PROGRESS (authored 2026-08-27 by Nash, approved by Connor; adversarially
audited 2026-08-27 by an independent fresh-context agent — verdict "sound with
amendments", all amendments folded into this text, marked "audit")
Branch: create a feature branch off `dev` (e.g. `refactor/026-socket-simplification`).
Trunk is `dev`. NEVER commit to or merge into `main`. No force pushes.

This is a **breaking release** (0.25.0 → 0.26.0 across the workspace). All three
phases land together in one release. Do the phases IN ORDER — each leaves the
workspace compiling and tests passing, and each gets its own commit(s).

---

## Progress

Nash keeps this checklist current as implementation proceeds. Each box is
checked only after that phase's validation gate passes and the work is
committed on `refactor/026-socket-simplification`.

- [x] **B1** — naia-server-socket concrete types (commit `124b4af7`)
- [x] **B2** — naia-client-socket concrete types (commit `975e34da`)
- [x] **B3** — IdentityToken opaque byte newtype (commit pending)
- [ ] **C1** — one `HandshakeHeader`
- [ ] **C2** — one client handshaker
- [ ] **C3** — one server handshaker
- [ ] **C4** — feature rename `advanced_handshake` → `address_validation`
- [ ] **C5** — full validation gate + docs
- [ ] **D** — version bump 0.25.0 → 0.26.0 + CHANGELOG

Known pre-existing failure, unrelated to this mission and NOT to be "fixed"
here: `naia-benches` `bench_protocol::tests::halo_unit_facing_wraps` panics in
`shared/src/world/component/property.rs` ("mutable HostOwned Property mutated
before its mutator was installed"). It fails identically before any of this
work. Everything else in `cargo test --workspace` passes.

---

## Ground rules (read before touching anything)

- Canonical repo: `~/Work/specops/naia`. If you need a worktree, it MUST sit
  beside a `slag` checkout (the workspace has relative path deps like
  `../slag`); `~/Work/specops/nash-home/` works, `/tmp` does not.
- Cyberlith consumes naia via **local path deps** from `~/Work/specops/naia`.
  Do not break the chain: webrtc-unreliable-client native flow →
  naia-client-socket → naia-client. All work happens on a branch, so canonical
  `dev` stays safe until merge.
- Never run bare `git stash` / `git stash pop` (shared stash stack). Use
  `git stash push -u -m "<tag>"`, note the SHA, `git stash apply <sha>`.
- `cargo fmt --all` / `cargo clippy --fix --all` traverse path deps ACROSS repo
  boundaries here. Only format files you edited (`rustfmt --edition 2021 <file>`).
- The pre-push hook runs the full workspace build + wasm checks and takes
  several minutes. Give push commands a long timeout (10 min).
- Disk is chronically near-full on this machine. If a build fails with
  "no space left on device", tell Connor; don't delete things outside this repo.
- Known pre-existing test failure (NOT yours to fix, do not be alarmed):
  `naia-benches` `bench_protocol::tests::halo_unit_facing_wraps` panics at
  `shared/src/world/component/property.rs:427`. It fails on `dev` before your
  changes. Everything else must pass.

### Validation gate (run after EVERY phase)

```
cargo check --workspace
cargo check -p naia-client --features transport_webrtc
cargo check -p naia-client --features transport_udp
cargo check -p naia-server --features transport_webrtc
cargo check -p naia-server --features transport_udp
cargo check -p naia-client --features transport_webrtc,transport_udp
cargo check -p naia-server --features transport_webrtc,transport_udp
cargo check -p naia-client-socket --features wbindgen --target wasm32-unknown-unknown
cargo test --workspace
```

(Expect only the pre-existing `halo_unit_facing_wraps` failure.)

---

## Background: what exists today (verified 2026-08-27)

### Two parallel trait layers — only ONE gets removed

1. **naia-level transport traits** in `client/src/transport/mod.rs` and
   `server/src/transport/mod.rs` (`PacketSender`, `PacketReceiver`,
   `IdentityReceiver`, etc., used as `Box<dyn ...>` in `connection/io.rs`,
   `server/main_server.rs`, conditioners, and the udp/webrtc/local transport
   impls). **These STAY.** They are the plug point that lets naia-client /
   naia-server accept UDP, WebRTC, or local transports interchangeably.
2. **socket-crate traits** in `naia-client-socket` and `naia-server-socket`.
   **These are the removal target (Phase B).** They exist for no reason:
   - Server (`socket/server/src/`): exactly one backend. `auth_sender.rs`,
     `auth_receiver.rs`, `packet_sender.rs`, `packet_receiver.rs` each define
     `pub trait Foo` + `pub trait FooClone` + `impl Clone for Box<dyn Foo>`
     boilerplate around a single `FooImpl` struct. `socket.rs` returns
     `(Box<dyn AuthSender>, Box<dyn AuthReceiver>, Box<dyn PacketSender>,
     Box<dyn PacketReceiver>)` and defines a `SocketTrait`.
   - Client (`socket/client/src/`): `packet_sender.rs`, `packet_receiver.rs`,
     `identity_receiver.rs` define the same trait+Clone-helper pattern. The
     backends (`backends/native/`, `backends/wasm_bindgen/`,
     `backends/miniquad/`) are **mutually exclusive by cfg** — exactly one
     compiles per platform — so a concrete type alias per cfg works fine.
     `conditioned_packet_receiver.rs` wraps `Box<dyn PacketReceiver>`.

### IdentityToken today

- `socket/shared/src/identity_token.rs`:
  `pub type IdentityToken = String;` and `generate_identity_token()` returns 32
  random lowercase ASCII chars.
- Generated in `server/src/server/main_server.rs:144` and `:319` via
  `naia_shared::generate_identity_token()` (re-exported at `shared/src/lib.rs:48`).
- Delivered to the client **out-of-band over HTTP signaling** on BOTH
  transports (WebRTC: `socket/server/src/session.rs` response; UDP:
  `client/src/transport/udp/auth.rs` ureq POST response, parsed as a string).
- Presented back **in-band** during handshake and disconnect, serialized with
  `Serde` (`.ser()` / `.de()` on String today).
- `server/src/handshake/advanced_handshaker.rs:17` has a private duplicate
  `type IdentityToken = String;` — must go.

### Handshakers today (the Phase C target)

The simple/advanced split is a **build-wide cfg_if either/or**, not per-connection:

- `shared/Cargo.toml:32` — `advanced_handshake = []` (empty feature).
- `shared/src/handshake/mod.rs` — cfg_if: feature on → `pub mod advanced`,
  off → `pub mod simple`. Each dir has its own `header.rs` with a DIFFERENT
  `HandshakeHeader` enum (simple: Identify/Connect/Disconnect + responses;
  advanced: Challenge/Validate/Connect/Disconnect + responses).
- `client/src/handshake/mod.rs` + `server/src/handshake/mod.rs` — cfg_if on
  `feature = "transport_udp"`: advanced_handshaker.rs vs simple_handshaker.rs,
  both exporting `HandshakeManager`. The `Handshaker` traits, `HandshakeResult`,
  and `HandshakeAction` live in these mod.rs files and are shared.
- `client/Cargo.toml:24` / `server/Cargo.toml:22` — `transport_udp` enables
  `naia-shared/advanced_handshake` (and ureq/base64/etc. on the client).
- `adapters/bevy/shared/Cargo.toml:17` also forwards `advanced_handshake`.

**Why both handshakers exist, and what's actually different** (this analysis is
settled — do not re-litigate it):

Both run the SAME session negotiation: present IdentityToken in-band → server
binds SocketAddr→UserKey → time-sync ping/pong rounds (`HandshakeTimeManager`)
→ connect request/response → `HandshakeResult::Connected(Box<TimeManager>)`.
Both carry ProtocolId and reject on mismatch. Both verify Disconnect packets
against the stored token.

The advanced one ADDS exactly one thing: a **source-address validation**
round-trip (Challenge/Validate). The server HMACs a timestamp
(`connection_hash_key`, `ring::hmac`), the client must echo the digest,
proving it owns its claimed source address (spoof-flood defense; see the
`CacheMap` LRU comments in `server/src/handshake/advanced_handshaker.rs`).
Raw UDP needs this because source addresses are forgeable. WebRTC doesn't —
ICE/DTLS already prove address ownership. This is NOT an alternative auth
mechanism; IdentityToken (identity) and the HMAC challenge (address
validation) have different roles, and both handshakers use IdentityToken
identically.

So: merge each pair into ONE handshaker where the challenge/validate stage is
compiled in iff the feature is enabled. Keep the build-wide semantic (feature
on = validation stages exist for the whole build); do NOT invent per-connection
runtime negotiation. The wire format per feature-configuration is unchanged in
shape (a merged header enum is fine because client and server in one deployment
always share the same build features — there is no cross-version compat
requirement in a breaking release).

Line counts for orientation: client simple 219 / advanced 280; server simple
186 / advanced 325. Expect the merge to delete roughly the ~500 duplicated
lines.

---

## PHASE B1 — remove socket-crate trait boxes (server first)

Work in `socket/server/src/`:

1. In `auth_sender.rs`, `auth_receiver.rs`, `packet_sender.rs`,
   `packet_receiver.rs`: delete the `pub trait Foo`, `pub trait FooClone`,
   `impl<T> FooClone for T`, and `impl Clone for Box<dyn Foo>` blocks. Keep
   each `FooImpl` struct; rename it to the plain name (`AuthSenderImpl` →
   `AuthSender`, etc.). Add `#[derive(Clone)]` if the manual clone plumbing
   provided it. Keep the existing inherent methods as `pub fn` with the same
   signatures the trait had.
2. `conditioned_packet_receiver.rs` (server): change its inner field from
   `Box<dyn PacketReceiver>` to the concrete `PacketReceiver` type. Because
   `Socket::listen*` previously chose conditioned-vs-plain at runtime by
   returning different boxed types, introduce a small enum in
   `packet_receiver.rs`:
   ```rust
   #[derive(Clone)]
   pub enum PacketReceiver { Plain(...), Conditioned(...) }
   ```
   with `receive()` matching on self — OR keep the conditioner as a wrapper
   struct and make the public receiver type the enum. Pick whichever compiles
   more simply; the public API just needs ONE concrete, Clone-able receiver
   type.
3. `socket.rs`: delete `SocketTrait`. Change `listen`/`listen_with_auth` (and
   friends) to return the concrete tuple
   `(AuthSender, AuthReceiver, PacketSender, PacketReceiver)` /
   `(PacketSender, PacketReceiver)`.
   Notes (audit-verified 2026-08-27): `SocketTrait` is dead code — never used
   as `dyn` or a generic bound anywhere; no mocks/test-doubles implement any of
   these traits; no consumer ever clones the boxed receivers/senders; the
   non-auth `Socket::listen` has zero in-repo callers (everything uses
   `listen_with_auth`) — keep it, but know no check exercises it.
   Scope note: the "conditioner enum" change applies ONLY to
   `socket/{client,server}/src/conditioned_packet_receiver.rs`. The separate
   naia-level conditioners (`client/src/transport/conditioner.rs`,
   `server/src/transport/conditioner.rs`) are OUT of scope.
4. `lib.rs`: update `pub use` exports to the concrete types.
5. Update consumers in `server/src/transport/webrtc.rs` (and
   `demos/socket/server/src/app.rs`): they currently take the boxed returns and
   wrap them into naia-server's own transport traits. They now hold the
   concrete socket types inside their adapter structs. The naia-level
   `Box<dyn ...>` transport layer above them DOES NOT CHANGE.
6. Run the validation gate. Commit: `naia-server-socket: concrete types, drop trait boxes`.

## PHASE B2 — same for naia-client-socket

Work in `socket/client/src/`:

1. Delete the trait + Clone-helper boilerplate in `packet_sender.rs`,
   `packet_receiver.rs`, `identity_receiver.rs`. These files then only need to
   re-export the per-backend concrete types. Also delete the client-side
   `SocketTrait` twin in `backends/socket.rs` (same dead-code pattern as B1).
2. The backends are cfg-exclusive. In each of `backends/native/`,
   `backends/wasm_bindgen/`, `backends/miniquad/`, rename `FooImpl` → `Foo`.
   At crate root (via the existing cfg_if in `backends/mod.rs` / `lib.rs`),
   re-export so that `naia_client_socket::PacketSender` etc. name exactly one
   concrete type per platform.
3. `conditioned_packet_receiver.rs`: same enum-or-wrapper treatment as B1
   step 2, so `Socket::connect*` returns one concrete receiver type.
4. `backends/*/socket.rs`: return concrete tuples instead of
   `(Box<dyn IdentityReceiver>, Box<dyn PacketSender>, Box<dyn PacketReceiver>)`.
5. Update consumers: `client/src/transport/webrtc.rs`,
   `demos/socket/client/app/src/app.rs`. Again, naia-client's own transport
   traits stay.
6. Validation gate, INCLUDING the wasm target check AND
   `cargo check -p naia-client-socket --features mquad --target wasm32-unknown-unknown`
   (miniquad backend is wasm-targeted; verify which target it cfg's to before
   assuming — check `backends/mod.rs` cfg_if — and use that target).
   Commit: `naia-client-socket: concrete types, drop trait boxes`.

## PHASE B3 — IdentityToken newtype

1. `socket/shared/src/identity_token.rs`: replace the alias with
   ```rust
   #[derive(Clone, PartialEq, Eq, Hash, Debug)]
   pub struct IdentityToken(Vec<u8>);
   ```
   with `impl` blocks providing: `generate()` (32 random bytes via the existing
   `Random` util — replaces free fn `generate_identity_token`, keep a
   deprecated-free re-export only if churn is large), `from_bytes(Vec<u8>)`,
   `as_bytes(&self) -> &[u8]`, and — because BOTH signaling paths ship the
   token over HTTP as text — `to_signaling_string(&self) -> String` /
   `from_signaling_string(&str) -> Option<Self>` using base64, URL-safe no-pad.
   IMPORTANT — base64 VERSION: the workspace pins `base64 = "0.13"` everywhere
   (client, server, socket/server, socket/client Cargo.tomls). Add
   `base64 = "0.13"` to `socket/shared/Cargo.toml` (do NOT use 0.21+, which
   would add a third base64 version to the lock and has a different API).
   The 0.13 API is `base64::encode_config(bytes, base64::URL_SAFE_NO_PAD)` /
   `base64::decode_config(s, base64::URL_SAFE_NO_PAD)`.
   NOTE — `Random` util (`socket/shared/src/backends/*/random.rs`) has no
   raw-byte generator; `generate()` should loop `gen_range_u32(0, 256) as u8`
   32 times.
   Also add `is_empty()` and/or `len()` accessors — the test harness asserts
   on token round-trips (see harness subsection below).
2. Implement `Serde` (naia's bit-serde) for it, delegating to `Vec<u8>`'s
   existing impl (`shared/serde/src/impls/vector.rs:8-16`). NOTE this is a
   deliberate wire-format change: String's Serde uses a
   `UnsignedVariableInteger<9>` length prefix, `Vec<u8>`'s uses `<5>`. That is
   fine in a breaking release — but say so in the CHANGELOG, don't treat it as
   a no-op. Serialization sites: client simple handshaker (identify request +
   `write_disconnect`), client advanced (challenge request), server simple
   `verify_disconnect_request` (`server/src/handshake/simple_handshaker.rs:162`
   has a second `IdentityToken::de()` — don't miss it), server advanced
   `recv_challenge_request`.
3. Fix every consumer. Known sites (STILL grep `IdentityToken` to be
   exhaustive — this list is audit-expanded but greps rot):
   - `server/src/server/main_server.rs:144,319` — call the new generator.
   - `server/src/handshake/advanced_handshaker.rs:17` — DELETE the private
     `type IdentityToken = String;`, import the real one.
   - Both server handshakers' `HashMap<IdentityToken, UserKey>` — works via
     derived Hash/Eq.
   - **`format!` interpolation sites (will NOT compile without rewrites —
     the newtype deliberately has no `Display`):**
     - `socket/server/src/session.rs:435` —
       `format!("{{\"sdp\":{body},\"id\":\"{identity_token}\"}}")` → use
       `to_signaling_string()`.
     - `server/src/transport/udp.rs:220` —
       `format!("{}\r\n{}", identity_token, self.public_udp_addr)` → same.
       (This whole file is a consumer — naia-server's UDP signaling responder.)
     - `server/src/transport/local/auth.rs:51` — same pattern.
   - HTTP/text parse boundaries → `from_signaling_string`:
     `client/src/transport/udp/auth.rs:133`;
     `socket/client/src/backends/wasm_bindgen/data_channel.rs`
     (`get_session_response()` JSON parse is the conversion point);
     `socket/client/src/backends/miniquad/shared.rs:36-46` (`receive_id`
     JsObject→String is the miniquad conversion point — NOT that backend's
     identity_receiver.rs, which only reads `ID_CELL`);
     `socket/client/src/backends/native/identity_receiver.rs` (receives
     `Result<String, u16>` from webrtc-unreliable-client — see note below).
   - Mechanical passthrough type fixes: `server/src/transport/webrtc.rs`,
     `socket/server/src/socket.rs`, `socket/server/src/async_socket.rs`,
     `socket/server/src/auth_*`, `client/src/transport/local/auth.rs`,
     `client/src/transport/local/inner_socket.rs`,
     `shared/src/transport/local/shared.rs`, `test/bevy_npa/src/world.rs`,
     `adapters/bevy/*`.
   - webrtc-unreliable-client boundary (audit-verified, NO wuc changes
     needed): wuc parses the signaling JSON `id` field as an opaque String
     with zero validation and hands it over its `Result<String, u16>` channel
     verbatim, so a base64 token flows through untouched. Convert with
     `from_signaling_string` on the naia side of the channel.
4. **Test harness (real design decision, NOT mechanical):**
   `test/harness/contract_tests/integration_only/01_connection_lifecycle.rs`
   has a tampered-token negative test (~line 1078:
   `format!("{}_tampered", token)`) and a round-trip assertion (~line 1271:
   `!token.is_empty()`). The tamper test cannot be expressed by mutating a
   validated `IdentityToken` value. Rewrite it to tamper at the byte level:
   build the tampered value via `IdentityToken::from_bytes(mutated_bytes)`
   (from_bytes is unvalidated by design — any byte vec is a syntactically
   valid token; "tampered" means it won't match the server's stored token,
   which is exactly what the test asserts). The `is_empty` assertion uses the
   new accessor. Preserve the test's INTENT (server cleanly rejects a token
   that doesn't match); do not delete or weaken it.
5. Validation gate. Commit: `IdentityToken: String -> opaque byte newtype`.

## PHASE C — merge the handshakers

### C1: one HandshakeHeader

1. In `shared/src/handshake/`, collapse `simple/` and `advanced/` into a single
   module: one `HandshakeHeader` enum containing the union of variants
   (Identify pair from simple; Challenge/Validate pairs from advanced; the
   shared Connect/Reject/Disconnect variants once). Gate the Challenge/Validate
   variants and their serde arms with `#[cfg(feature = "advanced_handshake")]`
   ONLY if keeping them unconditional breaks something; otherwise leave them
   unconditional (dead variants cost nothing and simplify the code).
   NOTE: compare the two `header.rs` files first — the simple one's
   `ClientIdentifyRequest` carries `ProtocolId`, the advanced one's
   `ClientChallengeRequest(ProtocolId)` does too. Preserve each variant's
   payload exactly.
2. Delete the cfg_if in `shared/src/handshake/mod.rs`; `reject_reason.rs`
   stays as-is.

### C2: one client handshaker

1. Merge `client/src/handshake/{simple,advanced}_handshaker.rs` into a single
   `handshaker.rs`. The state enum becomes:
   ```rust
   enum HandshakeState {
       #[cfg(feature = "transport_udp")] AwaitingChallengeResponse,
       #[cfg(feature = "transport_udp")] AwaitingValidateResponse,
       #[cfg(not(feature = "transport_udp"))] AwaitingIdentifyResponse,
       TimeSync(HandshakeTimeManager),
       AwaitingConnectResponse(TimeManager),
       Connected,
   }
   ```
   The TimeSync → Connected tail (time-sync pings, connect request, pong
   handling, `HandshakeResult::Connected(Box<TimeManager>)`, rejection
   handling, `write_disconnect` with token) is IDENTICAL in both files today —
   write it once. Only the entry stage(s) and their `send()`/`recv()` arms are
   cfg-gated. Diff the two files first to confirm the tail really matches
   (it does as of 2026-08-27; resolve trivial drift toward the advanced
   version).
2. **NOT part of the identical tail — must ALSO be cfg-gated under
   `transport_udp` (audit finding; missing these compiles wrong or not at
   all):**
   - struct fields `pre_connection_timestamp: Timestamp` and
     `pre_connection_digest: Option<Vec<u8>>` (advanced-only);
   - `write_disconnect()` — the two versions differ: simple serializes the
     identity token; advanced serializes the signed timestamp
     (`write_signed_timestamp`) and does NOT include the token. Keep BOTH
     bodies, cfg-gated, exactly as they are today.
3. `client/src/handshake/mod.rs`: delete the cfg_if, export the single
   `HandshakeManager`.

### C3: one server handshaker

1. Merge `server/src/handshake/{simple,advanced}_handshaker.rs` into
   `handshaker.rs`. Common core: the three user maps
   (`authenticated_and_identified_users`, `authenticated_unidentified_users`,
   `identity_token_map`), `authenticate_user`, `delete_user`,
   ProtocolMismatch rejection, token→UserKey binding →
   `HandshakeAction::FinalizeConnection`, ConnectRequest handling, verified
   Disconnect. cfg-gated under `transport_udp`: `connection_hash_key`
   (`ring::hmac`), `address_to_timestamp_map` + `timestamp_digest_map`
   (`CacheMap`, keep `cache_map.rs` gated), the Challenge/Validate arms, and
   `been_handshaked_users` (audit-confirmed advanced-only). Preserve the
   LRU-bound anti-flood comments and constants verbatim.
2. **Two match/function bodies differ STRUCTURALLY between simple and
   advanced — cfg-gate the WHOLE BODY of each, do not try to merge them
   incrementally (audit finding; a half-merge compiles but silently breaks
   connection finalization or disconnect verification in one config):**
   - `ClientConnectRequest` arm: simple returns
     `HandshakeAction::ForwardPacket` (finalize already happened at
     ClientIdentifyRequest); advanced is where `FinalizeConnection` actually
     fires, via a `been_handshaked_users` lookup populated during
     ClientValidateRequest. The two configs finalize on DIFFERENT header
     variants — preserve each behavior exactly.
   - `verify_disconnect_request`: simple = exact IdentityToken map compare;
     advanced = HMAC-signed-timestamp verification, no token involved. Two
     distinct trust models; keep both bodies cfg-gated.
3. `server/src/handshake/mod.rs`: delete the cfg_if. Answer (settled by
   audit): `HandshakeAction::ForwardPacket`'s
   `#[cfg_attr(feature = "transport_udp", allow(dead_code))]` remains correct
   after the merge IFF step 2's ClientConnectRequest gating is done as
   specified — keep the attribute, update its comment to reference the merged
   file.
4. Minor conscious choice: simple logs a `warn!` on ProtocolMismatch,
   advanced doesn't — pick one (keep the warn) and note it in the commit.
5. `ring` stays where it is: optional dep of naia-server under `transport_udp`
   (`server/Cargo.toml`, ring 0.17).

### C4: feature rename (do this LAST, it's mechanical)

Rename `advanced_handshake` → `address_validation` (the name now matches the
job). Sites: `shared/Cargo.toml:32`, `client/Cargo.toml:25`,
`server/Cargo.toml:23`, `adapters/bevy/shared/Cargo.toml:17`, and every
`#[cfg(feature = "advanced_handshake")]` you introduced in shared. Note the
handshaker cfgs in client/server gate on `transport_udp` (which ENABLES
naia-shared's feature) — inside naia-shared itself, gate on the shared
feature. Grep for stragglers: `grep -rn advanced_handshake --include='*.rs' --include='*.toml' .`

### C5: validate + docs

1. Full validation gate (all feature combos matter most here), PLUS these
   audit-added checks:
   ```
   cargo check -p naia-client --features transport_local
   cargo check -p naia-server --features transport_local
   cargo check -p naia-client --features transport_webrtc,transport_udp,transport_local
   cargo check -p naia-server --features transport_webrtc,transport_udp,transport_local
   cargo check -p naia-bevy-client --features transport_webrtc,transport_udp
   cargo check -p naia-bevy-server --features transport_webrtc,transport_udp
   ```
2. Update any docs/comments that mention "simple handshaker" / "advanced
   handshaker". Update this file's Status line to IMPLEMENTED.
3. Add a doc note (in `book/src/transports/udp.md` or
   `book/src/concepts/connection.md`): client and server binaries MUST agree
   on `transport_udp` on/off status to interoperate — `HandshakeHeader` is
   Serde-encoded by positional variant index, so simple-build clients cannot
   handshake with advanced-build servers. This is a PRE-EXISTING property the
   merge preserves unchanged (verified by audit); it just was never written
   down.
4. Commit per sub-phase (C1..C4) or as logically small commits.

## PHASE D — version bump + changelog

1. Bump `version = "0.25.0"` → `"0.26.0"` in all six core crates
   (`shared`, `client`, `server`, `socket/shared`, `socket/client`,
   `socket/server`) AND their intra-workspace dependency version fields.
2. ALSO bump the bevy adapters — this is unconditional, they DO carry
   versioned path deps: `adapters/bevy/shared/Cargo.toml`,
   `adapters/bevy/client/Cargo.toml`, `adapters/bevy/server/Cargo.toml`
   (plus `naia-bevy-derive` if versioned the same way; `naia-metrics` /
   `naia-bevy-metrics` are `publish = false` and excluded). Match the crate
   table in `_AGENTS/ARCHIVE/PUBLISH_PLAN.md` (the 0.25.0 release plan —
   NOTE: `_AGENTS/RELEASE_PROCESS.md` does NOT contain the publish order;
   only the archived plan does).
3. Add entries to the root `CHANGELOG.md` under `## [Unreleased]` /
   `### Breaking changes`, matching the existing format, for EACH of:
   socket-crate trait removal (public API of published crates
   naia-client-socket / naia-server-socket), IdentityToken String→byte
   newtype (including the wire-format length-prefix change and the base64
   signaling encoding), handshaker merge, and the `advanced_handshake` →
   `address_validation` feature rename. Mirror to
   `book/src/reference/changelog.md` if that's the convention for the other
   entries there.
4. Do NOT publish to crates.io — Connor decides release timing. Just make the
   tree version-consistent.
5. Final validation gate, push branch, tell Connor it's ready for review/merge
   into `dev`.

---

## What is explicitly OUT of scope

- No changes to the naia-level transport trait layer (`client/src/transport/`,
  `server/src/transport/`, `connection/io.rs`).
- No socket-API lifecycle/disconnect-event surfacing (Connor rejected the
  quick-death-detection motivation).
- No new crates (a `naia-client-connection` crate was considered and rejected —
  cfg gating inside existing crates is sufficient).
- No changes to `PacketType::Heartbeat` or the heartbeat/timeout logic
  (heartbeats carry the ack `StandardHeader` and must stay on all transports).
- No wuc (webrtc-unreliable-client) changes.
- Do not touch `demos/` beyond mechanical compile fixes.
