# Connection lifecycle SPEC honesty audit — Usher 112416 / Nash 113305

Ground truth is the row text in `01_connection_lifecycle.spec.md`.
For each row: the test that actually proves the obligation (by test NAME —
many `spec_expect` tags in the lifecycle file use a different row vocabulary
and are flagged `[TAG-MISMATCH]` where the tag names another row), the
verdict, and the fix commit where one was needed.

Gates for this audit: `cargo fmt --check` clean (pinned 1.98.0 toolchain),
`cargo clippy -p naia-server --all-targets -- -D warnings` clean,
`cargo clippy -p naia-test-harness --all-targets -- -D warnings` clean,
`integration_only_01_connection_lifecycle` 28/28,
`naia-server --lib` 106/106 default, 119/119 `transport_udp`
(incl. 4 new handshake unit tests).
Product fix + leg rewrites landed as `0bb68249` (FF `eb006054`..`0bb68249`
on both `dev` and `nash-dev`); this table file lands on top.

## Verdicts

| Row | Obligation (short) | Citing test(s) | Verdict |
|-----|-------------------|----------------|---------|
| 09 | no Naia-level auth timeout during transport handshake | `invalid_credentials_rejected`, `silent_server_surfaces_single_auth_timeout`, `give_up_then_redial_recovers_on_same_client` | HONEST (positive shape: auth decided pre-transport; give-up is generic link-silence AuthTimeout, exactly once) |
| 10 | identity token one-time-use, TTL ≤ 1h | `expired_or_reused_token_obeys_semantics` (one-time-use leg) + R1 `expired_token_at_identify_is_rejected_like_unknown`, `expired_token_at_challenge_is_answered_like_unknown`, `sweep_removes_only_expired_token_entries`, `fresh_token_at_identify_finalizes_once`, `fresh_token_at_challenge_answers` (server handshaker; TTL from `ServerConfig::identity_token_ttl`, default 3600 s; virtual-clock driven) | HONEST: on identify builds the aged token is refused like unknown; on UDP the challenge answers-as-usual (address validation only, binds nothing) and the judgment waits for validate, where the missing user binding draws the bounded Auth reject — no finalize on either path, sweep drops only expired grants |
| 11 | expired/used/invalid token → explicit reject | `malformed_identity_token_rejected`, `expired_or_reused_token_obeys_semantics` + R1 `expired_token_at_identify_is_rejected_like_unknown` (expired leg: Auth reject + pending user for deletion) + R1 `unknown_token_chain_ends_in_bounded_reject` (UDP: answered challenge + valid cookie with no bound user → bounded Auth reject, no larger than its trigger) | HONEST: expired takes the unknown-token refusal path per spec mechanism; on UDP the explicit reject is produced at validate, by then at a proven address |
| 12 | tokens required for ALL transports when require_auth | `invalid_credentials_rejected`, `malformed_identity_token_rejected` (e2e, local transport) + R1 `identify_unknown_token_emits_auth_reject`, `identify_unknown_token_rejects_pending_user_for_deletion` (identify build shared by local/WebRTC) + R1 `unknown_token_at_challenge_finalizes_nothing`, `unknown_token_chain_ends_in_bounded_reject` (UDP build: challenge answers, binds nothing, never finalizes; validate rejects) | HONEST: admission centralized above the Socket boundary (single `finalize_connection` route via token-consuming `HandshakeManager`; socket layer admits nothing); the token is judged at identify (local/WebRTC) or at validate after an answered challenge (UDP) — both emit the bounded explicit reject, pinned on both handshake builds |
| 13 | first validation consumes; replays fail | `expired_or_reused_token_obeys_semantics` (consumed T_A replayed → explicit reject with address) | HONEST |
| 14 | tick sync required; not Connected until sync | `connected_only_after_tick_sync_completes` (client handshaker R1: identify + 3 forged pong rounds yield None throughout; connect response after finished sync yields Connected) | HONEST |
| 14a | protocol_id checked first in handshake | `protocol_id_verified_before_connect_event` (matching passes) + `no_partial_protocol_compatibility` (mismatched client draws `ProtocolMismatch` reject, no `ConnectEvent` for it on either side — the check lands before connect) | HONEST: mismatch-rejected-pre-connect now proven by the row-33 test's mismatch leg |
| 15 | CLIENT ConnectEvent only at finalize | `no_connect_event_before_handshake_finalizes` (5-tick pre-accept window: no ClientConnectEvent, client non-connected; fires only after accept + completion) + R1 `connected_only_after_tick_sync_completes` (no Connected through identify + 3 pong rounds; Connected only on post-sync connect response, the direct `on_connect` input) | HONEST |
| 16 | SERVER ConnectEvent only at finalize | `no_connect_event_before_handshake_finalizes` (5-tick pre-accept window: no ServerConnectEvent; fires only after accept + completion) + R1 `fresh_token_at_challenge_finalizes_nothing` (UDP midpoint answers SendPacket, never FinalizeConnection) + identify-build token causality (finalize consumes the accept-minted token, pinned row 12) | HONEST |
| 17 | no replication before ConnectEvent | `no_replication_before_auth_decision` [TAG-MISMATCH: tagged 13.t1] | HONEST |
| 18 | explicit reject for no-token/bad-token/denied | `invalid_credentials_rejected`, `malformed_identity_token_rejected`, `server_reject_connection_produces_reject_event`, `server_reject_connection_carries_a_reason_message` + R1 `identify_empty_token_emits_auth_reject` (decodable empty → explicit Auth reject no larger than its trigger — anti-reflection bound pinned) + R1 `identify_without_token_bytes_finalizes_nothing`, `challenge_without_token_bytes_finalizes_nothing`, `challenge_empty_token_finalizes_nothing`, `unknown_token_chain_ends_in_bounded_reject` (UDP: answered challenge + valid cookie with no bound user → bounded Auth reject) | HONEST (Usher 115027 + 115062 implemented): wire token non-optional (undecodable → transport-04 drop, pinned); decodable empty/unknown → explicit Auth reject on identify builds (pinned + size bound); on UDP the challenge answers-as-usual (address only, FLIPS IF false — cookie carries no user binding) and validate judges at a proven address with the bounded reject (pinned full-chain). No silenced client hangs. One measured exception: the challenge-answer cookie necessarily exceeds a zero-length-token request — the cookie IS the mechanism; the bound holds for all token-bearing challenges and both reject paths |
| 19 | reject → RejectEvent, no ConnectEvent, no DisconnectEvent | `server_reject_connection_produces_reject_event` (reject + no-connect + 20-tick drain asserting no `ClientDisconnectEvent` for the never-connected attempt) | HONEST: DisconnectEvent-absence pinned — rejected B observes no disconnect across the pump window |
| 20 | status non-connected after reject, no special rejected status | `invalid_credentials_rejected` (02.t1: transitions to disconnected) | HONEST |
| 21 | t1: client DisconnectEvent ONLY if previously connected (every reason except give-up). t2 (Usher 113324): never-connected give-up gets exactly one AuthTimeout, zero ConnectEvents | `silent_server_surfaces_single_auth_timeout` pins t2 (exactly-one AuthTimeout + zero-ConnectEvent assertions); `give_up_then_redial_recovers_on_same_client` pins recovery | HONEST under ruling 113324. t1 negative (non-give-up reason without connect) still unasserted — sub-gap |
| 22 | server DisconnectEvent ONLY if previously connected | `client_disconnects_due_to_heartbeat_timeout` (positive: event after timeout-driven drop) + `server_reject_connection_produces_reject_event`, `malformed_identity_token_rejected` (negative: 20-tick `ServerDisconnectEvent` drains asserting none for the never-connected key — reject paths delete via `user_delete`/direct removal without queuing a disconnect) | HONEST: negative pinned on both reject paths |
| 23 | disconnect → scope out-of-scope + despawn owned | `basic_connect_disconnect_lifecycle` (A spawns `entity_a`, server replicates, A disconnects, bounded 40-tick wait asserts the server no longer has the entity) | HONEST: owned-entity despawn pinned — the old punt comment is removed, the server despawns on disconnect |
| 24 | server event order Auth→Connect→Disconnect | `connect_event_ordering_stable` + `basic_connect_disconnect_lifecycle` | HONEST combined |
| 25 | no-auth order Connect→Disconnect | `basic_connect_disconnect_lifecycle` (runs require_auth=false default) | HONEST |
| 26 | client order Connect→Disconnect | `client_disconnects_due_to_heartbeat_timeout`, `give_up_then_redial_recovers_on_same_client` | HONEST |
| 27 | reject → RejectEvent ONLY, no Connect/Disconnect | `malformed_identity_token_rejected` (explicit reject + no-connect + the connection-24 cleanliness pump extended to drain `ClientDisconnectEvent`, asserting none) | HONEST: DisconnectEvent-absence pinned on the tampered-token path |
| 28 | reconnect is a fresh session | `reconnect_is_fresh_session` | HONEST |
| 29 | protocol_id deterministic | `same_protocol_produces_same_id` + R1 `shared::protocol` id tests | HONEST (integration leg thin on id equality; R1 pins it directly) |
| 30 | protocol_id u128 little-endian wire form | R1 `shared::protocol_id::protocol_id_tests` (`the_wire_form_is_the_sixteen_bytes_in_order`, handshake round-trip, LE value/new) | HONEST at R1. Integration 30.t1 tag is circular (connect success ≠ encoding proof) |
| 31 | matched id proceeds; mismatched refused | `matched_protocol_id_allows_connection` | HONEST positive. Negative belongs to the hollow mismatch test below |
| 32 | wire-relevant aspects change the id | `protocol_id_determined_by_wire_relevant_aspects` (negctled both directions) | HONEST |
| 33 | exact match or reject, no partial compatibility | `no_partial_protocol_compatibility` (control: base-protocol client connects; mismatched client on `protocol_with_sequenced_unreliable` draws `RejectReason::ProtocolMismatch`, no `ConnectEvent` for it on either side) | HONEST: differing protocol tried for real — explicit mismatch reject, no connects, no negotiation |

## Hollow / vacuous legs found and fixed in `0bb68249`

- `disconnect_during_auth_handled_cleanly` (09): asserted no-user-mid-auth, false by
  design (user record is created at auth-drain). Rewritten: asserts no ConnectEvent
  + client down after cancel + late accept; B connects healthy.
- `concurrent_auth_requests_serialize` (10): `read_event`-per-call drain destroys
  the second same-tick auth. New harness `read_events` plural API; leg drains both.
- `no_mid_session_reauth` (15/16): helper consumed the handshake auth before the
  leg counted it; 16 fail-open on missed capture. Manual handshake + fail-shut capture.
- `malformed_identity_token_rejected` (23/24): previous version tampered the
  harness mirror before any token existed (`if let Some` never fired) and asserted
  `connected || rejected`. Rewritten over the proven pre-seed mechanism.
- `protocol_id_determined_by_wire_relevant_aspects` (32): never varied any wire
  aspect. Now asserts id equality + channel variation directly (negctled).
- Product gap (24): unknown-token identify left the user record behind (proven by
  probe: user `true` 20/20 ticks, client down, no event). Fixed with
  `HandshakeAction::RejectUser` + `unidentified_addresses` map; 3 new unit tests.
- `no_mid_session_reauth` 16 now fail-shut on capture.

## Hollow legs NOT fixed (reported, untouched)

- `protocol_handshake_mismatch_fails` (tags 21.t1/22.t1): 21.t1 connects a MATCHING
  protocol; 22.t1 is `Some(())`. Covers neither spec-21/22 nor the spec-31/spec-33
  negative. Proves nothing.
- `no_partial_protocol_compatibility` (33.t1): "verified implicitly", matching-only.
- `successful_auth_with_require_auth` 08.t1: unconditional `Some(())` (helper ate
  the auth event). Vacuous.
- `protocol_id_wire_encoding_allows_connection` 30.t1: circular (row covered at R1).
- Pervasive tag/spec drift: tags 13/15/16/17/18/19/21/22/23/24/25/26/27 (and 06)
  name obligations the spec text assigns to other rows. Table above is keyed by
  SPEC text, citing tests by NAME. No tags retagged — Usher vocabulary follows the
  tags; retagging needs a coordinator decision.

## Open gaps (oldest first)

1. Row 14 (tick-sync gating): CLOSED — `connected_only_after_tick_sync_completes`.
2. Row 10/11 TTL-by-age: CLOSED per Usher ruling 113643 (implement TTL) — 5
   handshaker pins + sweep + `ServerConfig::identity_token_ttl`.
3. Row 21: RESOLVED by Usher ruling 113324 — spec amended with t2, pin
   strengthened with zero-ConnectEvent assertion. Remaining sub-gap: t1 negative
   (non-give-up disconnect reason without prior connect) unasserted.
4. Row 18 no-token sub-case: CLOSED (Usher 115062 implemented) — UDP
   challenge answers-as-usual (address only, FLIPS IF false), validate
   judges at a proven address with the bounded Auth reject (full-chain
   R1 pin); rows 10/11/12 verdicts updated to the new contract.
5. Rows 19/27 DisconnectEvent-absence: CLOSED — 20-tick
   `ClientDisconnectEvent` drains asserting none on both the explicit
   server-reject path and the tampered-token path. Row 22 negative:
   CLOSED (row-22 commit — `ServerDisconnectEvent` drains on both
   reject paths). Row 23 owned-entity despawn: CLOSED — `basic` pins a
   bounded 40-tick wait for the server to drop A's replicated entity
   after A disconnects (no behavior change; the punt was stale).
5. Rows 15/16 absence-before: CLOSED — e2e pre-accept silence on both sides +
   R1 midpoint pins (client tick-sync gating, UDP challenge answers-only).
5. Row 12 multi-transport: CLOSED — admission centralized above the Socket
   boundary; no-token → no-finalize pinned on both handshake builds + e2e.
