# zstd Compression & Dictionary Training

naia supports optional **zstd packet compression** on a per-direction basis.
Compression is applied after naia's internal bit-packing and quantization, and
can be configured independently for each direction of the connection.

---

## Configuration

```rust
use naia_shared::{CompressionConfig, CompressionMode};

let compression = CompressionConfig::new(
    Some(CompressionMode::Default(3)),  // server → client, level 3
    None,                               // client → server, uncompressed
);
```

Three modes are available:

| Mode | When to use |
|------|-------------|
| `CompressionMode::Default(level)` | General use. Level −7 (fastest) to 22 (best ratio). Level 3 is a good starting point. |
| `CompressionMode::Dictionary(level, dict)` | Production. A custom dictionary trained on real game packets achieves 40–60% better compression than the default dictionary on typical game-state delta data. |
| `CompressionMode::Training(n_samples)` | Dictionary collection mode. Run for a representative play session; naia accumulates packet samples internally. |

---

## Dictionary training workflow

> **Tip:** A trained dictionary applied to your own game's packet shape typically achieves
> 40–60% better compression ratios than zstd's built-in defaults. The training
> step is a one-time cost.

1. Set `CompressionMode::Training(2000)` in your development build.
2. Run a representative play session (2000 packets ≈ a few minutes at 20 Hz).
3. Extract the trained dictionary from the server's `CompressionEncoder` and
   save it to a file (e.g. `assets/naia_dict.bin`).
4. Ship with:

```rust
CompressionMode::Dictionary(
    3,
    include_bytes!("../assets/naia_dict.bin").to_vec(),
)
```

---

## When to use compression

- **Use it** when bandwidth is the primary constraint (mobile clients, data-capped
  players, high entity counts).
- **Skip it** when CPU cost is more important than wire size (e.g. embedded
  servers, very high tick rates).

> **Note:** Compression applies to the full packet payload after bit-packing. The naia
> quantized numeric types (`SignedVariableFloat`, `UnsignedInteger<N>`, etc.)
> reduce the payload size before compression runs — combine both for maximum
> bandwidth efficiency.
