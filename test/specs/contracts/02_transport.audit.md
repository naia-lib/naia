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
| 01 | assume unordered/unreliable; no reliance on order/once/delivery | `03_messaging` conditioner hold/overtake leg (reorder: held update overtaken, stale dropped over a 48-tick window) + whole e2e suite treats the local link as unreliable | THIN: reorder tolerance proven at the messaging layer; drop has a conditioner knob (`incoming_loss`) with no nonzero-loss test, and duplicate has no knob or test — boundary-level drop/duplicate tolerance unpinned |
| 02 | MTU boundary is `naia_shared::MTU_SIZE_BYTES` | (constant definition + derivation in `shared/serde/src/constants.rs`, re-export; consumed as the per-packet send budget) | THIN: defined, derived (430 B worst-case envelope), and used as budget — but no test asserts the boundary role (no 431 B payload split/cap pin; accumulator tests hardcode 430) |
| 03 | oversize outbound attempt returns `Err` at Naia layer | none — `a_message_too_big_for_an_empty_packet_panics` (unit, `unordered_unreliable_sender`) asserts the unreliable path PANICS at write time, and neither error enum carries an oversize variant | HOLLOW: no test asserts `Err` from any initiating API on oversize; the one related pin proves a panic, the opposite of the obligation |
| 04 | malformed/oversize inbound dropped (Prod silent, Debug warn) | `remote_untrusted_input_does_not_panic` (garbage injected client→server, session survives = dropped) + `01` row-18 `challenge_without_token_bytes_finalizes_nothing` (undecodable challenge bytes → silent drop) | THIN: the drop is pinned; the Prod-silent/Debug-warn split is not (malformed-handshake sites call `warn!` unconditionally) |
| 05 | no transport-specific guarantees leak upward | (whole messaging/entity suites run on the local transport only) | THIN: true by construction, but no cross-transport differential test — UDP coverage is handshake-unit only, nothing runs the same assertions across transports |
| 06 | senders release attempt state on `shutdown`, idempotently; retry dials fresh | `shutdown_is_idempotent_and_preserves_finding` (R1, client `transport_udp` build: double shutdown, `Finding` preserved, send-while-finding errors) | HONEST: idempotent no-op teardown-drop pinned at the unit level on the UDP build |

## Gap items

1. Row 03 oversize-returns-Err: HOLLOW — needs a proving test (and possibly
   a behavior change, if the initiating API today panics instead of
   returning `Err`). Work row-by-row under Usher 115294.
2. Rows 01/02/04/05 THIN legs: drop/duplicate boundary pins, MTU boundary-role
   pin, Prod/Debug split pin, cross-transport differential evidence.
