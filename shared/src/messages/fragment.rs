use naia_derive::MessageFragment;
use naia_serde::{
    wire_schema_field, BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedInteger,
    WireSchema, WireSchemaContext,
};

const FRAGMENT_ID_BITS: u8 = 10;
const FRAGMENT_ID_LIMIT: u16 = 2_u16.pow(FRAGMENT_ID_BITS as u32);
const FRAGMENT_INDEX_BITS: u8 = 20;
const FRAGMENT_INDEX_LIMIT: u32 = 2_u32.pow(FRAGMENT_INDEX_BITS as u32);

// FragmentId
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FragmentId {
    inner: u16,
}

impl FragmentId {
    pub(crate) fn zero() -> Self {
        Self { inner: 0 }
    }

    pub(crate) fn increment(&mut self) {
        self.inner += 1;
        if self.inner >= FRAGMENT_ID_LIMIT {
            self.inner = 0;
        }
    }
}

impl Serde for FragmentId {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let integer = UnsignedInteger::<FRAGMENT_ID_BITS>::new(self.inner as u64);
        integer.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let integer = UnsignedInteger::<FRAGMENT_ID_BITS>::de(reader)?;
        let inner = integer.get() as u16;
        Ok(Self { inner })
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for FragmentId {
    fn const_bit_length() -> u32 {
        FRAGMENT_ID_BITS as u32
    }
}

// Schema descriptors //

// Transparent wrappers: each serializes exactly as its fixed-width
// unsigned integer, so each describes exactly as that integer (the bit
// width travels with it — re-bitting either type changes its descriptor).
impl WireSchema for FragmentId {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        wire_schema_field::<UnsignedInteger<FRAGMENT_ID_BITS>>(ctx, out);
    }
}

// FragmentIndex
#[derive(Copy, Clone, PartialEq)]
pub struct FragmentIndex {
    inner: u32,
}

impl FragmentIndex {
    pub(crate) fn zero() -> Self {
        Self { inner: 0 }
    }

    pub(crate) fn increment(&mut self) {
        self.inner += 1;
        if self.inner >= FRAGMENT_INDEX_LIMIT {
            panic!("Attempting to fragment large message, but hit fragment limit of {FRAGMENT_INDEX_LIMIT}. This means you're trying to transmit about 500 megabytes, which is a bad idea.")
        }
    }

    pub fn as_usize(&self) -> usize {
        self.inner as usize
    }
}

impl Serde for FragmentIndex {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let integer = UnsignedInteger::<FRAGMENT_INDEX_BITS>::new(self.inner as u64);
        integer.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let integer = UnsignedInteger::<FRAGMENT_INDEX_BITS>::de(reader)?;
        let inner = integer.get() as u32;
        Ok(Self { inner })
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for FragmentIndex {
    fn const_bit_length() -> u32 {
        FRAGMENT_INDEX_BITS as u32
    }
}

impl WireSchema for FragmentIndex {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        wire_schema_field::<UnsignedInteger<FRAGMENT_INDEX_BITS>>(ctx, out);
    }
}

#[cfg(test)]
mod schema_tests {
    use naia_serde::{WireSchema, SCHEMA_TAG_INTEGER, WIRE_SCHEMA_DOMAIN};

    use super::{FragmentId, FragmentIndex};

    /// Transparency is byte-identity with the underlying fixed-width
    /// integer; the two widths stay distinguishable.
    #[test]
    fn fragment_wrappers_describe_as_their_integers() {
        assert_eq!(
            &FragmentId::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_INTEGER, 0, 0, 10],
        );
        assert_eq!(
            &FragmentIndex::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_INTEGER, 0, 0, 20],
        );
        assert_ne!(
            FragmentId::wire_schema_bytes(),
            FragmentIndex::wire_schema_bytes(),
            "re-bitting either wrapper must differ",
        );
    }
}

#[derive(MessageFragment)]
pub struct FragmentedMessage {
    id: FragmentId,
    index: FragmentIndex,
    total: FragmentIndex,
    bytes: Box<[u8]>,
}

impl FragmentedMessage {
    pub fn new(id: FragmentId, index: FragmentIndex, bytes: Box<[u8]>) -> Self {
        Self {
            id,
            index,
            bytes,
            total: FragmentIndex::zero(),
        }
    }

    pub(crate) fn set_total(&mut self, total: FragmentIndex) {
        self.total = total;
    }

    pub(crate) fn id(&self) -> FragmentId {
        self.id
    }

    pub(crate) fn index(&self) -> FragmentIndex {
        self.index
    }

    pub(crate) fn total(&self) -> FragmentIndex {
        self.total
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_payload(self) -> Box<[u8]> {
        self.bytes
    }
}

#[cfg(test)]
impl FragmentIndex {
    /// Builds an index/total directly, for tests that need to construct the
    /// values a hostile peer could put on the wire.
    pub(crate) fn from_u32(inner: u32) -> Self {
        Self { inner }
    }
}
