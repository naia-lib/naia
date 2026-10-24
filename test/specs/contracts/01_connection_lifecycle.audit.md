# Connection lifecycle SPEC honesty audit — Usher 112416 / Nash 113305

Ground truth is the row text in `01_connection_lifecycle.spec.md`.
For each row: the test that actually proves the obligation (by test NAME —
many `spec_expect` tags in the lifecycle file use a different row vocabulary
and are flagged `[TAG-MISMATCH]` where the tag names another row), the
verdict, and the fix commit where one was needed.

Gates for this audit: `cargo fmt --check` clean (pinned 1.98.0 toolchain),
`cargo clippy -p naia-server --all-targets -- -D warnings` clean,
`cargo clippy -p naia-test-harness --all-targets -- -D warnings` clean,
`integration_only_01_connection_lifecycle` 27/27,
`naia-server --lib` 105/105 (incl. 3 new handshake unit tests).
Product fix + leg rewrites landed as `0bb68249` (FF `eb006054`..`0bb68249`
on both `dev` and `nash-dev`); this table file lands on top.

## Verdicts

| Row | Obligation (short) | Citing test(s) | Verdict |
|-----|-------------------|----------------|---------|
| 09 | no Naia-level auth timeout during transport handshake | `invalid_credentials_rejected`, `silent_server_surfaces_single_auth_timeout`, `give_up_then_redial_recovers_on_same_client` | HONEST (positive shape: auth decided pre-transport; give-up is generic link-silence AuthTimeout, exactly once) |
| 10 | identity token one-time-use, TTL ≤ 1h | `expired_or_reused_token_obeys_semantics` | HONEST for one-time-use. TTL-by-age UNCITED, no test exercises expiry |
| 11 | expired/used/invalid token → explicit reject | `malformed_identity_token_rejected`, `expired_or_reused_token_obeys_semantics` | HONEST for used/invalid. Expired-by-age shares the row-10 TTL gap |
| 12 | tokens required for ALL transports when require_auth | whole file (harness transport only) | THIN: proven for one transport; "all transports" unproven |
| 13 | first validation consumes; replays fail | `expired_or_reused_token_obeys_semantics` (consumed T_A replayed → explicit reject with address) | HONEST |
| 14 | tick sync required; not Connected until sync | none isolated | UNCITED (implicit only via `is_connected` asserts). Oldest uncited row: next work |
| 14a | protocol_id checked first in handshake | `protocol_id_verified_before_connect_event` | THIN: matching passes; mismatch-rejected-pre-connect unproven (see hollow mismatch test) |
| 15 | CLIENT ConnectEvent only at finalize | `connect_event_ordering_stable` | THIN: observes connect after accept; absence-before unasserted |
| 16 | SERVER ConnectEvent only at finalize | `connect_event_ordering_stable` | THIN: same as 15, server side |
| 17 | no replication before ConnectEvent | `no_replication_before_auth_decision` [TAG-MISMATCH: tagged 13.t1] | HONEST |
| 18 | explicit reject for no-token/bad-token/denied | `invalid_credentials_rejected`, `malformed_identity_token_rejected`, `server_reject_connection_produces_reject_event`, `server_reject_connection_carries_a_reason_message` | HONEST for bad-token/denied. No-token-with-require_auth sub-case UNCITED |
| 19 | reject → RejectEvent, no ConnectEvent, no DisconnectEvent | `server_reject_connection_produces_reject_event`, `malformed_identity_token_rejected` | HONEST except DisconnectEvent-absence never asserted (sub-gap) |
| 20 | status non-connected after reject, no special rejected status | `invalid_credentials_rejected` (02.t1: transitions to disconnected) | HONEST |
| 21 | client DisconnectEvent ONLY if previously connected | `silent_server_surfaces_single_auth_timeout`, `give_up_then_redial_recovers_on_same_client` | CONTESTED: both prove ClientDisconnectEvent(AuthTimeout) with NO prior ConnectEvent. Product violates the row as written, or give-up AuthTimeout is out of scope. Ruling needed, no code change made |
| 22 | server DisconnectEvent ONLY if previously connected | `client_disconnects_due_to_heartbeat_timeout` | THIN: positive proven (event after timeout-driven drop); negative (no event without connect) unasserted |
| 23 | disconnect → scope out-of-scope + despawn owned | `basic_connect_disconnect_lifecycle`, `client_disconnects_due_to_heartbeat_timeout` | HONEST for user/scope removal. Owned-entity despawn explicitly punted in `basic` comment (sub-gap) |
| 24 | server event order Auth→Connect→Disconnect | `connect_event_ordering_stable` + `basic_connect_disconnect_lifecycle` | HONEST combined |
| 25 | no-auth order Connect→Disconnect | `basic_connect_disconnect_lifecycle` (runs require_auth=false default) | HONEST |
| 26 | client order Connect→Disconnect | `client_disconnects_due_to_heartbeat_timeout`, `give_up_then_redial_recovers_on_same_client` | HONEST |
| 27 | reject → RejectEvent ONLY, no Connect/Disconnect | `malformed_identity_token_rejected`, `expired_or_reused_token_obeys_semantics` | HONEST except DisconnectEvent-absence unasserted (same sub-gap as 19) |
| 28 | reconnect is a fresh session | `reconnect_is_fresh_session` | HONEST |
| 29 | protocol_id deterministic | `same_protocol_produces_same_id` + R1 `shared::protocol` id tests | HONEST (integration leg thin on id equality; R1 pins it directly) |
| 30 | protocol_id u128 little-endian wire form | R1 `shared::protocol_id::protocol_id_tests` (`the_wire_form_is_the_sixteen_bytes_in_order`, handshake round-trip, LE value/new) | HONEST at R1. Integration 30.t1 tag is circular (connect success ≠ encoding proof) |
| 31 | matched id proceeds; mismatched refused | `matched_protocol_id_allows_connection` | HONEST positive. Negative belongs to the hollow mismatch test below |
| 32 | wire-relevant aspects change the id | `protocol_id_determined_by_wire_relevant_aspects` (negctled both directions) | HONEST |
| 33 | exact match or reject, no partial compatibility | `no_partial_protocol_compatibility` | HOLLOW: admits "verified implicitly", only connects a matching protocol, never tries a differing one |

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

1. Row 14 (tick-sync gating): no isolated test — next work per 113305.
2. Row 10/11 TTL-by-age: no expiry test.
3. Row 21: contested, needs ruling (product emits AuthTimeout disconnects for
   never-connected clients; `silent_server` + `give_up_redial` pin this behavior).
4. Row 18 no-token sub-case; rows 19/27 DisconnectEvent-absence; row 22 negative;
   row 23 owned-entity despawn; rows 15/16 absence-before; row 12 multi-transport.
