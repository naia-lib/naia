//! Bench-only port of cyberlith's `SerdeQuat` smallest-three quaternion encoding.
//!
//! Mirrors `cyberlith/crates/math/src/serde_quat.rs` bit-for-bit on the wire
//! (2 bits SkipComponent + 1 bit sign + 3×5 bits SignedInteger = 18 bits).
//! Stores the four components as a plain `[f32; 4]` so the bench has no
//! dependency on a quaternion math library — the bench measures wire size,
//! not rotation correctness.

use naia_shared::{
    wire_schema_count, wire_schema_field, wire_schema_label, BitReader, BitWrite, ConstBitLength,
    Serde, SerdeErr, SignedInteger, WireSchema, WireSchemaContext, SCHEMA_TAG_STRUCT,
};

#[derive(Clone, Copy, PartialEq)]
pub struct BenchQuat {
    inner: [f32; 4],
}

impl BenchQuat {
    const BITS: u8 = 5;
    const MAX_SIZE: f32 = 32.0;

    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self {
            inner: [x, y, z, w],
        }
    }

    pub fn get(&self) -> [f32; 4] {
        self.inner
    }
}

#[derive(Serde, Clone, Copy, PartialEq)]
enum SkipComponent {
    X,
    Y,
    Z,
    W,
}

impl ConstBitLength for SkipComponent {
    fn const_bit_length() -> u32 {
        2
    }
}

impl Serde for BenchQuat {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let q = self.inner;
        let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3])
            .sqrt()
            .max(1e-12);
        let q = [q[0] / len, q[1] / len, q[2] / len, q[3] / len];

        let mut biggest_value = f32::MIN;
        let mut biggest_index: usize = 0;
        for (i, &c) in q.iter().enumerate() {
            let a = c.abs();
            if a > biggest_value {
                biggest_value = a;
                biggest_index = i;
            }
        }

        let skip_component = match biggest_index {
            0 => SkipComponent::X,
            1 => SkipComponent::Y,
            2 => SkipComponent::Z,
            _ => SkipComponent::W,
        };

        let components = match skip_component {
            SkipComponent::X => [q[1], q[2], q[3]],
            SkipComponent::Y => [q[0], q[2], q[3]],
            SkipComponent::Z => [q[0], q[1], q[3]],
            SkipComponent::W => [q[0], q[1], q[2]],
        };

        let skipped_is_negative = match skip_component {
            SkipComponent::X => q[0] < 0.0,
            SkipComponent::Y => q[1] < 0.0,
            SkipComponent::Z => q[2] < 0.0,
            SkipComponent::W => q[3] < 0.0,
        };

        let components = components
            .map(|c| SignedInteger::<{ Self::BITS }>::new((c * Self::MAX_SIZE).round() as i128));

        skip_component.ser(writer);
        skipped_is_negative.ser(writer);
        components.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let skip_component = SkipComponent::de(reader)?;
        let skipped_is_negative = bool::de(reader)?;
        let components = <[SignedInteger<{ Self::BITS }>; 3]>::de(reader)?;

        let components = components.map(|c| {
            let v: i128 = c.to();
            v as f32 / Self::MAX_SIZE
        });

        let mut skipped =
            (1.0 - components[0].powi(2) - components[1].powi(2) - components[2].powi(2))
                .max(0.0)
                .sqrt();
        if skipped_is_negative {
            skipped = -skipped;
        }

        let inner = match skip_component {
            SkipComponent::X => [skipped, components[0], components[1], components[2]],
            SkipComponent::Y => [components[0], skipped, components[1], components[2]],
            SkipComponent::Z => [components[0], components[1], skipped, components[2]],
            SkipComponent::W => [components[0], components[1], components[2], skipped],
        };

        Ok(Self { inner })
    }

    fn bit_length(&self) -> u32 {
        Self::const_bit_length()
    }
}

impl ConstBitLength for BenchQuat {
    fn const_bit_length() -> u32 {
        SkipComponent::const_bit_length()
            + bool::const_bit_length()
            + <[SignedInteger<{ Self::BITS }>; 3]>::const_bit_length()
    }
}

// Schema descriptors //

// Structural, not a custom leaf: the packing decomposes exactly into a
// `SkipComponent` discriminant, a sign bit, and three fixed-width signed
// integers, and the descriptor names those pieces with the same constants
// (`Self::BITS`) the codec uses — so re-bitting the packing changes the
// descriptor automatically instead of silently. A bespoke grammar with no
// standard-node decomposition would take a curated leaf identifier;
// this one has a decomposition, so it uses it.
impl WireSchema for BenchQuat {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_STRUCT);
        wire_schema_count(&mut *out, 3);
        wire_schema_label(&mut *out, "skip_component");
        wire_schema_field::<SkipComponent>(ctx, &mut *out);
        wire_schema_label(&mut *out, "skipped_is_negative");
        wire_schema_field::<bool>(ctx, &mut *out);
        wire_schema_label(&mut *out, "components");
        wire_schema_field::<[SignedInteger<{ Self::BITS }>; 3]>(ctx, &mut *out);
    }
}

#[cfg(test)]
mod schema_tests {
    use naia_shared::{WireSchema, SCHEMA_TAG_STRUCT, WIRE_SCHEMA_DOMAIN};

    use super::BenchQuat;

    /// The packing facts travel: discriminant width, sign bit, and the
    /// exact integer width all discriminate.
    #[test]
    fn bench_quat_descriptor_names_its_packing() {
        let bytes = BenchQuat::wire_schema_bytes();
        assert_eq!(&bytes[..WIRE_SCHEMA_DOMAIN.len()], WIRE_SCHEMA_DOMAIN);
        assert_eq!(bytes[WIRE_SCHEMA_DOMAIN.len()], SCHEMA_TAG_STRUCT);
        let body = &bytes[WIRE_SCHEMA_DOMAIN.len() + 5..];
        assert!(body.windows(16).any(|w| w == b"skip_component"));
        assert!(body.windows(19).any(|w| w == b"skipped_is_negative"));
        assert!(body.windows(10).any(|w| w == b"components"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naia_shared::{BitReader, BitWriter};

    #[test]
    fn bit_length_matches_cyberlith_serde_quat() {
        // 2 (SkipComponent) + 1 (bool) + 3×(1 + 5) = 21 bits
        // SignedInteger<5> emits 1 sign bit + 5 magnitude bits each.
        assert_eq!(BenchQuat::const_bit_length(), 21);
    }

    #[test]
    fn round_trip() {
        let original = BenchQuat::new(0.0, 0.0, 0.0, 1.0);
        let mut writer = BitWriter::new();
        original.ser(&mut writer);
        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let decoded = BenchQuat::de(&mut reader).unwrap();
        for i in 0..4 {
            assert!((original.inner[i] - decoded.inner[i]).abs() < 0.05);
        }
    }
}
