use super::{bit_reader::BitReader, bit_writer::BitWrite, error::SerdeErr};

/// A trait for objects that can be serialized to a bitstream.
pub trait Serde: Sized + Clone + PartialEq {
    /// Serialize Self to a BitWriter
    fn ser(&self, writer: &mut dyn BitWrite);

    /// Parse Self from a BitReader
    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr>;

    /// Return length of value in bits
    fn bit_length(&self) -> u32;
}

pub trait ConstBitLength {
    fn const_bit_length() -> u32;
}

/// Sentinel returned for a type whose serialized width has no static bound —
/// anything holding a `Vec`, a `String`, a variable-length number, or an
/// `EntityProperty` (whose encoding is per-connection). Callers must treat it as
/// "unknown", never as a real width: `ComponentKinds::add_component` *skips* its
/// size assert when it sees this rather than failing, so an unbounded component
/// is admitted at registration and instead panics inside `world_writer`'s
/// `capture()` the first time it serializes past the cache.
pub const UNBOUNDED_BIT_LENGTH: u32 = u32::MAX;

/// Static-width probe with a fallback, for generated code that must ask "does
/// `T` have a const bit length?" without knowing whether `T: ConstBitLength`.
///
/// Rust has no stable specialization and a derive macro cannot inspect a field
/// type's trait impls, so this uses the standard *inherent-method-priority*
/// technique: [`MaxBits::<T>::probe`] exists as an inherent method only under
/// `T: ConstBitLength`, and method resolution prefers an inherent method over a
/// trait one. When the bound holds, the inherent method wins and reports the
/// real width; when it does not, the inherent method is not applicable and the
/// blanket [`MaxBitsFallback`] impl answers [`UNBOUNDED_BIT_LENGTH`].
///
/// Both paths are `const`-foldable no-ops at runtime — the whole probe compiles
/// to a constant.
pub struct MaxBits<T>(core::marker::PhantomData<T>);

impl<T> MaxBits<T> {
    pub const fn new() -> Self {
        Self(core::marker::PhantomData)
    }
}

impl<T> Default for MaxBits<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ConstBitLength> MaxBits<T> {
    /// The inherent (preferred) arm: `T` is statically bounded.
    pub fn probe(&self) -> u32 {
        T::const_bit_length()
    }
}

/// The fallback arm of [`MaxBits`] — see its docs. Must be in scope for the
/// probe to resolve for unbounded types.
pub trait MaxBitsFallback {
    fn probe(&self) -> u32 {
        UNBOUNDED_BIT_LENGTH
    }
}

impl<T> MaxBitsFallback for MaxBits<T> {}

#[cfg(test)]
mod max_bits_tests {
    use super::{ConstBitLength, MaxBits, MaxBitsFallback, UNBOUNDED_BIT_LENGTH};

    #[test]
    fn a_bounded_type_reports_its_real_width() {
        assert_eq!(MaxBits::<bool>::new().probe(), bool::const_bit_length());
        assert_eq!(MaxBits::<u8>::new().probe(), 8);
        // Composition is what the derive actually leans on.
        assert_eq!(MaxBits::<[Option<u8>; 4]>::new().probe(), 4 * (1 + 8));
    }

    #[test]
    fn an_unbounded_type_falls_back_to_the_sentinel() {
        // `Vec<T>` has no ConstBitLength impl, so the inherent arm does not
        // apply and the blanket trait answers.
        assert_eq!(MaxBits::<Vec<u8>>::new().probe(), UNBOUNDED_BIT_LENGTH);
        assert_eq!(MaxBits::<String>::new().probe(), UNBOUNDED_BIT_LENGTH);
    }
}
