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

// ── Canonical schema descriptors (Tier-B `WireSchema`) ────────────────────

use std::any::TypeId;

/// Domain-separation tag for the canonical schema-descriptor encoding.
///
/// Every top-level descriptor (see [`WireSchema::wire_schema_bytes`]) begins
/// with these bytes, so a schema descriptor can never be confused with a
/// fingerprint preimage, a packet, or another grammar's bytes.
pub const WIRE_SCHEMA_DOMAIN: &[u8] = b"naia:wire-schema:v1";

/// Fixed `u8` node tags for the canonical descriptor encoding. Hand-pinned:
/// a reorder must never silently move them.
pub const SCHEMA_TAG_STRUCT: u8 = 0x01;
pub const SCHEMA_TAG_ENUM: u8 = 0x02;
pub const SCHEMA_TAG_TUPLE: u8 = 0x03;
pub const SCHEMA_TAG_OPTION: u8 = 0x04;
pub const SCHEMA_TAG_ARRAY: u8 = 0x05;
pub const SCHEMA_TAG_VECTOR: u8 = 0x06;
pub const SCHEMA_TAG_HASH_MAP: u8 = 0x07;
pub const SCHEMA_TAG_HASH_SET: u8 = 0x08;
pub const SCHEMA_TAG_STRING: u8 = 0x09;
pub const SCHEMA_TAG_BOOL: u8 = 0x0A;
pub const SCHEMA_TAG_UNIT: u8 = 0x0B;
pub const SCHEMA_TAG_CHAR: u8 = 0x0C;
pub const SCHEMA_TAG_INTEGER: u8 = 0x0D;
pub const SCHEMA_TAG_FLOAT: u8 = 0x0E;
pub const SCHEMA_TAG_NATIVE: u8 = 0x0F;
pub const SCHEMA_TAG_CUSTOM_LEAF: u8 = 0x10;
pub const SCHEMA_TAG_ENTITY_PROPERTY: u8 = 0x11;
pub const SCHEMA_TAG_BACKREF: u8 = 0x12;
pub const SCHEMA_TAG_PHANTOM: u8 = 0x13;
pub const SCHEMA_TAG_BYTES: u8 = 0x14;

/// Collection order classes. Hand-pinned like the node tags.
pub const SCHEMA_ORDERED: u8 = 0x00;
pub const SCHEMA_UNORDERED: u8 = 0x01;

/// The describing host's endianness, resolved at compile time: `0x00` little,
/// `0x01` big. Native-endian primitives (`u16`…`f64`, `char`, `usize`) genuinely
/// disagree across endian hosts, so the descriptor records the fact instead of
/// pretending the bytes mean the same everywhere.
pub const SCHEMA_NATIVE_ENDIAN: u8 = if cfg!(target_endian = "big") {
    0x01
} else {
    0x00
};

/// A type's canonical structural schema descriptor.
///
/// This is a *type-level* fact: every value of `Self` shares one descriptor,
/// so the methods take no `self`. Descriptors are deterministic — the same
/// types produce the same bytes on every host with the same endianness — and
/// structural: registry fingerprints consume them instead of Rust names,
/// token text, or source-order accidents.
///
/// # Canonical encoding
///
/// All counts and labels are framed with `u32` little-endian lengths; numeric
/// metadata is fixed-width. Per node, after the tag byte:
/// ```text
/// STRUCT          field-count:u32, per field: label-len:u32 + label + descriptor
/// ENUM            bits-needed:u8, variant-count:u32, per variant:
///                   ordinal:u32, label-len:u32 + label,
///                   has-payload:u8, payload descriptor iff 0x01
/// TUPLE           elem-count:u32, then each element descriptor
/// OPTION          inner descriptor (the present bit is fixed grammar)
/// ARRAY           length:u32, then the element descriptor (no length on wire)
/// VECTOR          order-class:u8 (0x00 ordered), elem descriptor,
///                   length-codec descriptor
/// HASH_MAP        order-class:u8 (0x01 unordered), key + value descriptors,
///                   length-codec descriptor
/// HASH_SET        order-class:u8 (0x01 unordered), elem descriptor,
///                   length-codec descriptor
/// STRING          length-codec descriptor
/// BYTES           length-codec descriptor
/// BOOL | UNIT | PHANTOM   nothing (fixed single-bit / zero-bit grammar)
/// CHAR            endian:u8 (4 native bytes on the wire)
/// INTEGER         signed:u8, variable:u8, bits:u8
/// FLOAT           signed:u8, variable:u8, bits:u8, fraction-digits:u8
/// NATIVE          width-bytes:u8, signed:u8, float:u8, endian:u8
/// CUSTOM_LEAF     id-len:u32 + id bytes (curated stable identifier)
/// ENTITY_PROPERTY length-codec descriptor for the entity id
///                   (the present bit, host/remote bit, static bit, reversal,
///                   and UnsignedVariableInteger<7> id are fixed grammar)
/// BACKREF         traversal ordinal:u32 (see [`WireSchemaContext`])
/// ```
///
/// `Box<T>` is transparent (it emits `T`'s descriptor unchanged).
/// `PhantomData<T>` emits `PHANTOM` and does not recurse.
pub trait WireSchema: 'static {
    /// Appends this type's canonical descriptor to `out`, using `ctx` to
    /// fold recursive occurrences into back-references. Implementations emit
    /// composite children through [`wire_schema_field`], never by recursing
    /// directly, so active traversals always terminate.
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>);

    /// The complete self-contained descriptor for `Self`, prefixed with
    /// [`WIRE_SCHEMA_DOMAIN`].
    fn wire_schema_bytes() -> Vec<u8> {
        let mut ctx = WireSchemaContext::new();
        let mut out = Vec::new();
        out.extend_from_slice(WIRE_SCHEMA_DOMAIN);
        ctx.enter_root::<Self>();
        Self::wire_schema(&mut ctx, &mut out);
        ctx.exit::<Self>();
        out
    }
}

/// Tracks the active descriptor traversal so recursive types terminate.
///
/// The stack holds the `TypeId` of every type whose descriptor is currently
/// being emitted, outermost first. A field whose type is already on the stack
/// emits only its stack ordinal ([`SCHEMA_TAG_BACKREF`]); otherwise the field
/// pushes its type, emits the full descriptor, and pops on unwind. Ordinals
/// are therefore canonical traversal positions — deterministic across hosts —
/// and a `TypeId` never enters the descriptor bytes. Sibling (non-active)
/// occurrences re-emit their full descriptor; only live recursion folds.
#[derive(Debug, Default)]
pub struct WireSchemaContext {
    stack: Vec<TypeId>,
}

impl WireSchemaContext {
    /// An empty traversal.
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Records entering `T`: returns the existing ordinal for a live
    /// recursion, or pushes `T` and returns `None` for a full emission.
    fn enter<T: ?Sized + 'static>(&mut self) -> Option<u32> {
        let id = TypeId::of::<T>();
        if let Some(position) = self.stack.iter().position(|active| *active == id) {
            Some(position as u32)
        } else {
            self.stack.push(id);
            None
        }
    }

    /// Records leaving `T` after a full emission. Must pair with a prior
    /// `None` from [`enter`](Self::enter).
    fn exit<T: ?Sized + 'static>(&mut self) {
        let id = self
            .stack
            .pop()
            .expect("schema traversal stack underflow: exit without enter");
        debug_assert_eq!(
            id,
            TypeId::of::<T>(),
            "schema traversal exited out of order",
        );
    }

    /// Pushes the traversal root. The root always emits fully: a type that
    /// contains itself still describes its top level before any back-ref.
    fn enter_root<T: ?Sized + 'static>(&mut self) {
        debug_assert!(
            self.stack.is_empty(),
            "schema traversal root entered mid-traversal",
        );
        self.stack.push(TypeId::of::<T>());
    }

    /// A traversal already rooted at `T`: the root emits fully, and any
    /// recursion back to `T` folds to `BACKREF 0`.
    ///
    /// This is the entry point for generated standalone descriptors
    /// (Message and Replicate `wire_schema()` overrides). It requires only
    /// `T: 'static` for the `TypeId` — never `T: WireSchema`, which the
    /// generated overrides deliberately do not impose — because the root's
    /// own emission is the caller's inline code, not a trait call.
    /// Starting from `new()` instead would leave the root unseeded: a
    /// self-containing type would inline one full spurious level before
    /// anything folded.
    pub fn rooted<T: ?Sized + 'static>() -> Self {
        let mut ctx = Self::new();
        ctx.enter_root::<T>();
        ctx
    }
}

/// Emits one field/element descriptor: a back-reference when `T` is already
/// being described up-stack, otherwise `T`'s full descriptor. Generated and
/// hand-written `WireSchema` impls route every composite child through here.
pub fn wire_schema_field<T: WireSchema + ?Sized>(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
    if let Some(ordinal) = ctx.enter::<T>() {
        out.push(SCHEMA_TAG_BACKREF);
        out.extend_from_slice(&ordinal.to_le_bytes());
    } else {
        T::wire_schema(ctx, out);
        ctx.exit::<T>();
    }
}

/// Appends a `u32` little-endian count.
pub fn wire_schema_count(out: &mut Vec<u8>, count: u32) {
    out.extend_from_slice(&count.to_le_bytes());
}

/// Appends a length-framed label (`len:u32 LE` + bytes).
pub fn wire_schema_label(out: &mut Vec<u8>, label: &str) {
    wire_schema_count(out, label.len() as u32);
    out.extend_from_slice(label.as_bytes());
}

/// Emits a custom-leaf node: the tag plus a curated stable identifier. For
/// hand-written `Serde` impls whose grammar is bespoke (biased bits,
/// domain-specific packing), the identifier pins the grammar by reference —
/// the hand-written `ser`/`de` is the specification, and any change to it
/// must change the identifier.
pub fn wire_schema_custom_leaf(out: &mut Vec<u8>, id: &str) {
    out.push(SCHEMA_TAG_CUSTOM_LEAF);
    wire_schema_label(out, id);
}

#[cfg(test)]
mod schema_traversal_tests {
    use super::{wire_schema_field, WireSchemaContext, SCHEMA_TAG_BACKREF};

    /// Live recursion folds to stack ordinals; unwinding pops, so later
    /// siblings emit fully again. Ordinals are traversal positions, and no
    /// `TypeId` ever reaches the bytes.
    #[test]
    fn live_recursion_folds_to_ordinals_and_unwinds_cleanly() {
        let mut ctx = WireSchemaContext::new();

        // Root u8 at ordinal 0; a u8 field while active is a back-edge.
        ctx.enter_root::<u8>();
        let mut out = Vec::new();
        wire_schema_field::<u8>(&mut ctx, &mut out);
        assert_eq!(out, [SCHEMA_TAG_BACKREF, 0, 0, 0, 0]);

        // Mutual recursion: bool pushed at 1, then u8 and bool edges fold.
        assert_eq!(ctx.enter::<bool>(), None);
        assert_eq!(ctx.enter::<u8>(), Some(0));
        assert_eq!(ctx.enter::<bool>(), Some(1));
        ctx.exit::<bool>();

        // Unwind fully; the stack is reusable and a fresh field emits fully.
        ctx.exit::<u8>();
        let mut full = Vec::new();
        wire_schema_field::<u8>(&mut ctx, &mut full);
        assert_eq!(&full, &[super::SCHEMA_TAG_INTEGER, 0, 0, 8]);
    }
}

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
