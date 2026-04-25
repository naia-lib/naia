# Phase 9.2 — `naia_serde` derive bit-budget audit + proptest harness

**Status:** ✅ COMPLETE 2026-04-25 — audit found no sibling off-by-ones; proptest harness pins the bit-budget invariant for every derive shape and the `SerdeInteger` family; 17/17 new tests pass.

## What was audited

Phase 8.3 fixed a 5-year-old off-by-one in `bits_needed_for(N)` inside `enumeration.rs`. To confirm no sibling bugs lurked, every place a bit count is computed in the serde stack was static-read:

| File | Result |
|---|---|
| `shared/serde/derive/src/impls/structure.rs` | ✅ no overhead — `bit_length()` is sum of fields, `ser()` matches |
| `shared/serde/derive/src/impls/tuple_structure.rs` | ✅ no overhead — same shape as named struct |
| `shared/serde/derive/src/impls/enumeration.rs` | ✅ already fixed in Phase 8.3 — `bits_needed_for` correct |
| `shared/serde/src/number.rs` (`SerdeNumberInner::bit_length` vs `ser`) | ✅ fixed-width and variable-width paths bit-count consistent — verified by exhaustive boundary tests |
| `shared/serde/src/impls/scalars.rs` (`impl_serde_for!`) | ✅ raw-byte serialization; `bit_length` returns `<Self as ConstBitLength>::const_bit_length()` matching byte count |

No new off-by-ones found. The Phase 8.3 fix was the only outstanding bug in the bit-budget contract.

## What the harness pins

New file: `shared/serde/tests/bit_budget.rs` (17 tests, ~250 cases via proptest).

The single contract under test:

> For every `T: Serde`, `T.bit_length()` returns exactly the number of bits
> `T.ser()` writes, and `de(ser(x))` round-trips equal.

Coverage:

| Concern | Test |
|---|---|
| Named-field struct: bits = sum-of-fields, no padding | `struct_named_bit_length_matches_ser_and_round_trips`, `prop_named_struct_roundtrips` |
| Tuple struct: same | `struct_tuple_bit_length_matches_ser_and_round_trips`, `prop_tuple_struct_roundtrips` |
| Empty struct: 0 bits | `empty_struct_emits_zero_bits` |
| Enum tag widths at every power-of-2 boundary (N=1, 2, 3, 4, 5, 7, 8, 9, 16, 17, 32, 33) | `enum_tag_widths_match_ceil_log2_at_every_boundary` |
| Enum with mixed payloads (unit, tuple, named) | `enum_with_payloads_bit_length_matches_ser_and_round_trips`, `prop_enum_with_payloads_roundtrips` |
| `UnsignedInteger<N>` / `SignedInteger<N>` round-trip + `ConstBitLength` agreement | `unsigned_fixed_width_*`, `signed_fixed_width_*`, `prop_*_fixed_width_roundtrips`, `const_bit_length_agrees_with_bit_length_for_fixed_types` |
| `UnsignedVariableInteger<N>` boundary cases (0, last-fits-1-block, first-needs-2-blocks, …) | `unsigned_variable_bit_length_matches_ser_at_known_boundaries`, `prop_unsigned_variable_3_roundtrips` |
| `SignedVariableInteger<N>` round-trip + bit_length consistency | `prop_signed_variable_4_roundtrips` |

Every assertion checks **two** things — `bit_length() == bits actually written` AND `ser-then-de preserves equality`. The first catches off-by-ones; the second catches encode/decode asymmetries. Mutual reinforcement.

## Notable findings (not bugs)

- **`bit_length() == 1` for 1-variant enums.** `bits_needed_for(1)` floors at 1 because `UnsignedInteger<0>` is rejected at construction. A 1-variant enum could in principle encode in 0 bits, but the structural constraint costs 1 bit of overhead per encode. Not material in practice (1-variant enums are rare).
- **Plain `i16` / `u32` / etc. carry no sign-bit prefix.** The `impl_serde_for!` macro emits raw bytes — `i16` is 16 bits flat, regardless of value. Only the `SignedInteger<N>` family carries an explicit sign-bit + magnitude. The harness encodes both expectations.

## Companion change

Added a load-bearing doc comment at the top of `shared/serde/derive/src/lib.rs` stating the bit-budget invariant and pointing to the proptest harness. Future readers (and AI agents) see the contract immediately.

## Files touched

| File | Change |
|---|---|
| `shared/serde/Cargo.toml` | Added `proptest = "1"` dev-dep |
| `shared/serde/tests/bit_budget.rs` | **NEW** — 17-test harness pinning the bit-budget invariant |
| `shared/serde/derive/src/lib.rs` | Doc comment stating the invariant + pointer to harness |

## Verification

- ✅ `cargo test -p naia-serde --test bit_budget` — 17 passed (256 proptest cases / property)
- ✅ `cargo test --workspace` — 0 failures
- ✅ namako BDD gate green (carried from 9.1 — no wire change in 9.2)
- 🔄 29/0/0 wins gate: re-running on post-9.1+9.2 build

## Risk

Pure additive coverage. No production-code change beyond a doc comment. Zero wire impact.
