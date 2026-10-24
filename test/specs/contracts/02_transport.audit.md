# Transport SPEC honesty audit — Usher 115294 / Nash

Ground truth is the row text in `02_transport.spec.md`.
For each row: the test that actually proves the obligation (by test NAME),
the verdict, and the fix commit where one was needed.

Gates for this audit: `cargo fmt --check` clean (pinned 1.98.0 toolchain),
`cargo clippy -p naia-test-harness --all-targets -- -D warnings` clean,
`integration_only_01_connection_lifecycle` 28/28 (row-33 pin),
`naia-client --lib` 48/48 `transport_udp`.
No product change in this file's first landing; pins cited are pre-existing
unless a fix commit is named.

## Verdicts

| Row | Obligation (short) | Citing test(s) | Verdict |
|-----|-------------------|----------------|---------|
| 01 | assume unordered/unreliable; no reliance on order/once/delivery | `reliable_delivery_survives_packet_loss` (new `02_transport` target: 20% server→client loss, reliable message still arrives, session healthy after) + `03_messaging` conditioner hold/overtake leg (reorder) + `duplicates_and_stale_indices_are_still_ignored` (shared unit: duplicates/stale ignored) | HONEST: drop pinned e2e at the boundary (loss verifiably applied in `process_packet`; 6/6 green), reorder pinned e2e, duplicate pinned at the receiver unit level |
| 02 | MTU boundary is `naia_shared::MTU_SIZE_BYTES` | `mtu_boundary_matches_documented_derivation` (serde unit: 430 B == 576−60−8−50−28, bits == 3440) + `default_writer_holds_exactly_one_mtu` (serde unit: default `BitWriter` holds exactly 430 B / 3440 bits, packet slice is 430 B) + `default_writer_refuses_byte_past_the_mtu` (serde unit: byte 431 panics `Write overflow!`) | HONEST: value, derivation, and boundary role all pinned — the default writer IS the per-packet budget (cap `MTU_SIZE_BITS`, buffer `MTU_SIZE_BYTES`), exactly one MTU fits and the 431st byte refuses |
| 03 | oversize outbound attempt returns `Err` at Naia layer | `oversize_unreliable_send_returns_err` (new `02_transport` target: 5000 B body on unreliable → `Err`, same body on reliable → `Ok` + session unaffected) | HONEST: the manager refuses bodies over `FRAGMENTATION_LIMIT_BITS` on non-fragmenting channels (`false` → `Err(MessageQueueFull)` on both client and server APIs), which covers everything over MTU; no behavior change, the panic site below is unreachable through the API |
| 04 | malformed/oversize inbound dropped (Prod silent, Debug warn) | `remote_untrusted_input_does_not_panic` (garbage injected client→server, session survives = dropped) + `01` row-18 `challenge_without_token_bytes_finalizes_nothing` (undecodable challenge bytes → silent drop) | THIN: the drop is pinned; the Prod-silent/Debug-warn split is not (malformed-handshake sites call `warn!` unconditionally) |
| 05 | no transport-specific guarantees leak upward | (whole messaging/entity suites run on the local transport only) | THIN: true by construction, but no cross-transport differential test — UDP coverage is handshake-unit only, nothing runs the same assertions across transports |
| 06 | senders release attempt state on `shutdown`, idempotently; retry dials fresh | `shutdown_is_idempotent_and_preserves_finding` (R1, client `transport_udp` build: double shutdown, `Finding` preserved, send-while-finding errors) | HONEST: idempotent no-op teardown-drop pinned at the unit level on the UDP build |

## Gap items

1. Row 03 oversize-returns-Err: CLOSED — `oversize_unreliable_send_returns_err`
   pins `Err` on the initiating API (client leg; the harness server facade
   swallows the `Result`, documented in the test). The manager-level refusal
   predates the audit; no behavior change.
2. Rows 01/04/05 THIN legs: drop/duplicate boundary pins, Prod/Debug split
   pin, cross-transport differential evidence. (Row 02 MTU boundary-role:
   CLOSED — derivation + exactly-430-holds + byte-431-refuses, all serde
   unit level; no behavior change, the cap predates the audit.)
