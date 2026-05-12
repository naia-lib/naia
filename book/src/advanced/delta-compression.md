# Delta Compression

naia uses per-field delta compression to minimize the bandwidth cost of
replication updates. Only fields that actually changed are included in each
outbound packet — unchanged fields are never sent.

---

## How `Property<T>` works

`Property<T>` is naia's change-detection wrapper. When a field inside
`Property<T>` is mutated via `DerefMut`, the containing entity is marked dirty
and only the changed fields are included in the next `send_all_packets` call.
naia tracks per-field diffs for each in-scope user independently.

```rust
#[derive(Replicate)]
pub struct Position {
    pub x: Property<f32>,
    pub y: Property<f32>,
}

// Mutating through DerefMut marks the component dirty:
position.x.set(42.0);
// Only `x` is dirty — `y` is not sent this tick.
```

---

## Compact numeric types

`Property<T>` is generic over any `T: Serde`. naia ships a set of compact
numeric types in `naia_shared` that reduce wire size compared to raw `f32`/`u32`:

| Type | Wire size | Use case |
|------|-----------|----------|
| `UnsignedInteger<N>` | exactly N bits | health (0–255 → 8 bits), flags |
| `SignedInteger<N>` | exactly N bits | relative offsets |
| `UnsignedVariableInteger<N>` | 1–N bits (varint) | counts that are usually small |
| `SignedVariableInteger<N>` | 1–N bits (varint) | deltas that are usually near zero |
| `UnsignedFloat<BITS, FRAC>` | exactly BITS bits | positive position, speed |
| `SignedFloat<BITS, FRAC>` | exactly BITS bits | signed angle, velocity axis |
| `SignedVariableFloat<BITS, FRAC>` | 1–BITS bits | per-tick deltas (often tiny) |

`BITS` is the total bit width; `FRAC` is the number of decimal digits of
precision retained.

---

## Example — a quantized game unit

```rust
use naia_shared::{Property, Replicate, SignedVariableFloat, UnsignedInteger};

#[derive(Clone, PartialEq, Serde)]
pub struct PositionState {
    pub tile_x: i16,
    pub tile_y: i16,
    pub dx: SignedVariableFloat<14, 2>,  // 14-bit max, 2 decimal digits
    pub dy: SignedVariableFloat<14, 2>,  // encodes near-zero deltas in ~3 bits
}

#[derive(Replicate)]
pub struct Position {
    pub state: Property<PositionState>,
}
```

Wrapping multi-axis state in a single `Property<State>` means one dirty-bit
covers all axes — the whole struct is sent or nothing is, which is correct for
coupled state and avoids partial-update edge cases.

Compared to `Property<f32> × 4` (128 bits/tick), `PositionState` costs roughly
32 bits (2 × i16) + ~6–28 bits (variable delta) = **38–60 bits/tick** when
typical sub-tile movement is small — a 2–3× wire reduction.

> **Tip:** For position data that changes by small deltas each tick (smooth movement),
> `SignedVariableFloat` or `SignedVariableInteger` can encode near-zero values in
> as few as 3–4 bits, vs. 32 bits for a bare `f32`. Profile your actual packet
> sizes with the benchmark suite (`cargo bench -p naia_bench`) before and after
> to verify the gains.

See `benches/src/bench_protocol.rs` for working examples of `PositionQ`,
`VelocityQ`, and `RotationQ` using these types in a real benchmark scenario.

---

## Static entities — no delta tracking

[Static entities](../concepts/replication.md#static-vs-dynamic-entities) skip
delta tracking entirely. When a static entity enters scope, naia sends a full
component snapshot. After that no further updates are transmitted. Use them for
any entity that is written once and never changes — map tiles, level geometry, etc.
