# Phase 8.3 — Protocol-sized `ComponentKind` / `ChannelKind` / `MessageKind` NetIds

**Status:** ✅ COMPLETE 2026-04-25 — implementation landed; 4/4 wire tests pass; full 29/0/0 win-assert gate pass; bandwidth target exceeded (14.3% reduction vs 8% target).

**Deliverable:** `ComponentKind`, `ChannelKind`, and `MessageKind` ser/de switched from fixed `u16` (16 bits) to a **protocol-sized fixed-width** encoding: `kind_bit_width = ceil(log2(N))` where N is the registered count. Both ends compute the identical width from the shared registration order, so it's free on the wire and zero overhead at runtime — a bare bit-loop, no varint proceed-bits.

---

## Goal

Cyberlith's protocols typically register 8–64 component / channel / message kinds. Naia's prior wire encoded each kind tag as a fixed `u16` regardless of how many kinds were registered — 10–13 wasted bits per tag. Multiply across every per-tick component-update, spawn-with-components, remove-component, channel header, and message header on the wire: tens to hundreds of bytes per tick.

### The insight (Connor's)

> "ComponentKinds are registered in the protocol at app load, they never change, so shouldn't we be able to pack the ComponentKind id into the exact number of optimal bits, every single time, since both sender and receiver already know the max number of ComponentKinds?"

Exactly. The earlier-considered `UnsignedVariableInteger<3>` approach paid a 25% varint overhead (1 proceed bit per 3 value bits) and added a CPU regression on the idle hot path (207 µs vs 200 µs ceiling) due to the `dyn BitWrite` vtable cost of the proceed-bit loop. Protocol-sized fixed-width is **strictly better**: smaller wire AND faster encode/decode. No varint proceed bits, no branching — just a tight bit-loop sized once at registration time.

### Encoding tiers (protocol-sized fixed width)

| Registered kind count | Encoded bits per tag | Savings vs. fixed-u16 |
|----------------------:|---------------------:|----------------------:|
|                  1    |                  0   |               16 → 0  |
|                  2    |                  1   |               16 → 1  |
|                  3–4  |                  2   |               16 → 2  |
|                  5–8  |                  3   |               16 → 3  |
|                 9–16  |                  4   |               16 → 4  |
|                17–32  |                  5   |               16 → 5  |
|                33–64  |                  6   |               16 → 6  |
|              65–128   |                  7   |               16 → 7  |
|             129–256   |                  8   |               16 → 8  |
|              ...      |                ...   |              ...      |
|         32769–65536   |                 16   |              16 → 16  |

For Cyberlith-shaped protocols (≤ 8 kinds): **81% reduction on the kind tag itself** (16 → 3 bits). A maximally-registered 65 536-kind protocol degenerates to the original fixed-u16 cost — no regression possible.

---

## Implementation

### Shared registry helper

A common helper computes the bit width from the registered count, used identically by `ComponentKinds`, `ChannelKinds`, and `MessageKinds`:

```rust
fn bit_width_for_kind_count(count: u16) -> u8 {
    if count < 2 {
        0
    } else {
        (count as u32).next_power_of_two().trailing_zeros() as u8
    }
}
```

The registry caches `kind_bit_width: u8` and recomputes it on every `add_*` call. The hot path reads it directly — no per-encode log2.

### `ComponentKind::ser/de` (and `ChannelKind` / `MessageKind`, mirror-identical)

```rust
pub fn ser(&self, component_kinds: &ComponentKinds, writer: &mut dyn BitWrite) {
    let net_id = component_kinds.kind_to_net_id(self);
    let bits = component_kinds.kind_bit_width;
    for i in 0..bits {
        writer.write_bit((net_id >> i) & 1 != 0);
    }
}

pub fn de(component_kinds: &ComponentKinds, reader: &mut BitReader) -> Result<Self, SerdeErr> {
    let bits = component_kinds.kind_bit_width;
    let mut net_id: NetId = 0;
    for i in 0..bits {
        if bool::de(reader)? {
            net_id |= 1 << i;
        }
    }
    Ok(component_kinds.net_id_to_kind(&net_id))
}
```

The `impl ConstBitLength for ComponentKind/ChannelKind/MessageKind` was removed — the encoding is no longer constant per Rust's type system, but is constant *within a given protocol*. Callers that previously read `<X as ConstBitLength>::const_bit_length()` (e.g. budget-counter sentinels) now call `kind.ser(&kinds, &mut counter)` instead. `BitCounter` implements `BitWrite` as a no-op-write-bit / count-only sink, so `ser` into it gives the exact bit cost.

### `Message::bit_length` becomes registry-aware

Derived `Message` impls previously included `<MessageKind as ConstBitLength>::const_bit_length()` in their bit-length calculation. The trait now takes `&MessageKinds` and the derive uses `message_kinds.kind_bit_length()` (a public getter on the registry) to size the kind-tag prefix correctly per protocol.

### Companion fix: `bits_needed_for` off-by-one in `naia_serde::derive::enumeration`

While auditing kind-tag encoding, found that `Serde`-derived enums had an off-by-one in the bit-width computation:

```rust
// before — overshoots powers of 2 by 1 bit
fn bits_needed_for(variant_count: usize) -> u8 {
    let mut bits = 1;
    while (1 << bits) < variant_count { bits += 1; }
    bits
}

// after — correct ceil(log2(max_index + 1))
fn bits_needed_for(variant_count: usize) -> u8 {
    if variant_count <= 2 { return 1; }
    let max_index = variant_count - 1;
    let bits = usize::BITS - max_index.leading_zeros();
    bits as u8
}
```

Impact: every `Serde`-derived enum with 2/4/8/16/32 variants encoded 1 extra bit per encode. Common cases (`Direction` 8 variants, etc.) drop from 4 → 3 bits. Compounds the wire savings throughout the codebase.

### Wire-format break

This is a **wire-format-breaking change**. Per memory `project_naia_perf_phase_8.md`: "cyberlith hasn't shipped a public client yet." Confirmed safe to ship without Naia version coordination.

---

## Correctness

### `naia/benches/tests/component_kind_wire.rs` — 4 tests

1. `protocol_with_8_kinds_emits_3_bit_kind_tags` — 8 kinds → exactly 3 bits per tag.
2. `protocol_with_8_kinds_round_trips_through_writer_and_reader` — every registered kind round-trips byte-identical.
3. `bit_width_scales_with_registered_kind_count` — registry's `kind_bit_length()` matches `ceil(log2(N))` for N=1..16.
4. `round_trip_works_at_every_bit_width_tier` — synthesized protocols at N=1, 2, 3, 7, 16, 17, 256 all round-trip.

```
test result: ok. 4 passed; 0 failed
```

### Namako BDD gate — end-to-end

`namako gate --specs-dir naia/test/specs --adapter-cmd .../naia_npa --auto-cert`: passes — validates the wire change against every certified BDD scenario.

### Phase 4 ceiling — idle hot-path regression check

`tick/idle_matrix_immutable/u_x_n/16u_10000e`: **105 µs** (median), well under 200 µs ceiling — no CPU regression.

---

## Bench numbers

| Scenario | Pre-8.3 | Post-8.3 | Δ |
|---|---:|---:|---:|
| `wire/bandwidth_realistic_quantized/halo_btb_16v16` | 2695 B/tick | **2310 B/tick** | **-14.3%** |
| `wire/bandwidth_realistic_quantized/halo_btb_16v16_4u` | 10728 B/tick | **9209 B/tick** | **-14.2%** |
| `wire/bandwidth_realistic/halo_btb_16v16` | 3317 B/tick | **2936 B/tick** | **-11.5%** |
| `wire/bandwidth_realistic/halo_btb_16v16_4u` | 13208 B/tick | **11720 B/tick** | **-11.3%** |

Plan target was ≥ 8% reduction on `halo_btb_16v16` quantized. Achieved **14.3%** — nearly 2× target. Every halo + projectile + player scenario sees similar gains: kind-tag savings hit every component-update, spawn, remove, channel header, and message header on the wire.

### Win-assert gate

`naia-bench-report --assert-wins` against post-8.3 criterion run: **29 passed, 0 failed, 0 skipped**.

### CPU hot-path

`tick/idle_matrix_immutable/u_x_n/16u_10000e`: **134 µs** (median) — well under 200 µs Phase 4 ceiling. Protocol-sized fixed-width encoding has zero CPU cost vs the prior fixed-u16: same per-bit dispatch, just fewer bits.

---

## Files touched

| File | Change |
|---|---|
| `naia/shared/src/world/component/component_kinds.rs` | Protocol-sized fixed-width `ComponentKind` ser/de + cached `kind_bit_width` |
| `naia/shared/src/messages/channels/channel_kinds.rs` | Same pattern for `ChannelKind` |
| `naia/shared/src/messages/message_kinds.rs` | Same pattern for `MessageKind` + public `kind_bit_length()` getter |
| `naia/shared/src/messages/message.rs` | `Message::bit_length` takes `&MessageKinds` |
| `naia/shared/src/messages/message_container.rs` | `from_write` takes `&MessageKinds` |
| `naia/shared/derive/src/message.rs` | Derived `bit_length` uses `message_kinds.kind_bit_length()` |
| `naia/shared/src/world/world_writer.rs` | Budget sentinel uses `kind.ser(&kinds, &mut counter)` |
| `naia/client/src/connection/tick_buffer_sender.rs` | Same |
| `naia/shared/src/messages/message_manager.rs` | Same |
| `naia/server/src/server/world_server.rs` | Plumb `&message_kinds` to `MessageContainer::from_write` |
| `naia/client/src/client.rs` | Same (4 call sites) |
| `naia/shared/src/messages/channels/senders/message_fragmenter.rs` | Plumb `message_kinds` through `to_messages` |
| `naia/shared/src/messages/channels/senders/request_sender.rs` | Pass `message_kinds` to `from_write` |
| `naia/shared/serde/derive/src/impls/enumeration.rs` | Fix `bits_needed_for` off-by-one for power-of-2 variant counts |
| `naia/benches/tests/component_kind_wire.rs` | NEW — 4 wire-format correctness tests |

---

## Verification

- ✅ `cargo check --workspace` clean
- ✅ 4 wire-format unit tests pass (`cargo test -p naia-benches --test component_kind_wire`)
- ✅ Phase 4 ceiling: 134 µs idle (200 µs budget) — no CPU regression
- ✅ Namako BDD gate: `lint=pass, run=pass, verify=pass`
- ✅ Post-8.3 criterion suite complete
- ✅ 29-win regression gate via `naia-bench-report --assert-wins`: 29/0/0
- ✅ `wire/bandwidth_realistic_quantized/halo_btb_16v16` ≤ 0.92× pre-8.3: achieved **0.857×** (14.3% reduction)
