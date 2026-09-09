use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use naia_serde::{BitReader, BitWrite, Serde, SerdeErr, CACHED_UPDATE_BITS, UNBOUNDED_BIT_LENGTH};

use crate::world::component::replicate::SplitUpdateResult;
use crate::{
    LocalEntityAndGlobalEntityConverter, PendingComponentUpdate, Replicate, ReplicateBuilder,
};

type NetId = u16;

/// Wire encoding for `ComponentKind` NetIds is a fixed-width raw bit
/// field whose width is `ceil(log2(N))`, where N is the number of kinds
/// registered in the protocol. Both ends share the same registration
/// order, so both compute the same width and the encoding stays in sync.
///
/// This is strictly optimal: every tag is exactly the bits it needs, no
/// proceed-bit overhead, no varint loop. For a protocol with N kinds:
///
/// |     N      | Bits per tag |
/// |------------|-------------:|
/// |    0..1    |            0 |
/// |    2       |            1 |
/// |    3..4    |            2 |
/// |    5..8    |            3 |
/// |    9..16   |            4 |
/// |   17..32   |            5 |
/// |   33..64   |            6 |
/// |  65..128   |            7 |
/// | 129..256   |            8 |
///
/// Width is precomputed at registration time and cached on
/// `ComponentKinds`, so ser/de pays only an inline u8 read plus N
/// `write_bit` calls — no struct construction, no HashMap lookup for the
/// width. Pinned by `benches/tests/component_kind_wire.rs`.
fn bit_width_for_kind_count(count: NetId) -> u8 {
    // count <= 1 → 0 bits (degenerate; nothing to disambiguate).
    // count   N → ceil(log2(N)) bits.
    if count < 2 {
        0
    } else {
        (count as u32).next_power_of_two().trailing_zeros() as u8
    }
}

/// ComponentKind - should be one unique value for each type of Component
#[derive(Eq, Hash, Copy, Clone, PartialEq, Debug)]
pub struct ComponentKind {
    type_id: TypeId,
}

impl From<TypeId> for ComponentKind {
    fn from(type_id: TypeId) -> Self {
        Self { type_id }
    }
}
impl From<ComponentKind> for TypeId {
    fn from(val: ComponentKind) -> Self {
        val.type_id
    }
}

impl ComponentKind {
    /// Returns the `ComponentKind` corresponding to the type `C`.
    pub fn of<C: Replicate>() -> Self {
        Self {
            type_id: TypeId::of::<C>(),
        }
    }

    /// Serializes this kind's compact net-ID into `writer` using the bit-width in `component_kinds`.
    pub fn ser(&self, component_kinds: &ComponentKinds, writer: &mut dyn BitWrite) {
        let net_id = component_kinds.kind_to_net_id(self);
        let bits = component_kinds.kind_bit_width;
        for i in 0..bits {
            writer.write_bit((net_id >> i) & 1 != 0);
        }
    }

    /// Deserializes a `ComponentKind` from `reader` using the bit-width in `component_kinds`.
    pub fn de(component_kinds: &ComponentKinds, reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let bits = component_kinds.kind_bit_width;
        let mut net_id: NetId = 0;
        for i in 0..bits {
            if bool::de(reader)? {
                net_id |= 1 << i;
            }
        }
        component_kinds.net_id_to_kind(&net_id)
    }
}

/// Structural wire-compatibility facts for one registered component type.
///
/// The canonical domain descriptor ([`descriptor`](Self::descriptor)) covers
/// field layout; everything else that decides whether two peers agree on
/// what a component's bytes mean lives here as discrete facts, stored at
/// registration so the fingerprint compares values, never types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentFacts {
    /// `C::protocol_name()`.
    pub name: String,
    /// `C::wire_schema()` bytes verbatim.
    pub descriptor: Vec<u8>,
    /// From the registered builder's `is_immutable()`: the component is
    /// written once on spawn and never diff-tracked.
    pub immutable: bool,
    /// Ordered labels of the wired (replicated) properties, in diff-mask
    /// index order — the same list and order as the descriptor's STRUCT
    /// entries.
    pub property_labels: Vec<String>,
    /// Real diff-mask bit index per wired property, parallel to
    /// `property_labels`. Sparse declaration-order discriminants, not a
    /// dense renumbering.
    pub mask_indices: Vec<u8>,
    /// Diff-mask size in bytes over ALL declared fields.
    pub mask_size_bytes: u8,
    /// `C::has_entity_properties()`: serialized bytes differ per connection.
    pub has_entity_properties: bool,
    /// Labels of the `EntityProperty` fields, in declaration order: which
    /// labeled properties are entity relations.
    pub entity_property_labels: Vec<String>,
    /// Whether this component type can participate in delegated authority:
    /// it has at least one wired property AND is not immutable (an
    /// immutable component never sends updates, so there is nothing to
    /// delegate). The delegation mechanism itself is uniform across types;
    /// this flag is the per-type capacity fact.
    pub authority_delegable: bool,
}

/// A map to hold all component types
pub struct ComponentKinds {
    current_net_id: NetId,
    /// Number of bits needed to encode any registered NetId — recomputed
    /// on every `add_component` so it always reflects the current count.
    /// Read directly by `ComponentKind::ser`/`de` on the hot path.
    kind_bit_width: u8,
    kind_map: HashMap<ComponentKind, (NetId, Box<dyn ReplicateBuilder>, String)>,
    net_id_map: HashMap<NetId, ComponentKind>,
    /// Components where `has_entity_properties() == true` — their serialized bytes
    /// differ per connection and cannot use the shared CachedComponentUpdate cache.
    user_dependent: HashSet<ComponentKind>,
    /// Structural facts per registered component, keyed by kind. Stored at
    /// registration from the type's static schema methods so the fingerprint
    /// can compare field layout without re-walking the types.
    facts: HashMap<ComponentKind, ComponentFacts>,
}

impl Clone for ComponentKinds {
    fn clone(&self) -> Self {
        let current_net_id = self.current_net_id;
        let kind_bit_width = self.kind_bit_width;
        let net_id_map = self.net_id_map.clone();
        let user_dependent = self.user_dependent.clone();

        let mut kind_map = HashMap::new();
        for (key, value) in self.kind_map.iter() {
            kind_map.insert(*key, (value.0, value.1.box_clone(), value.2.clone()));
        }

        Self {
            current_net_id,
            kind_bit_width,
            kind_map,
            net_id_map,
            user_dependent,
            facts: self.facts.clone(),
        }
    }
}

impl Default for ComponentKinds {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentKinds {
    /// Creates an empty `ComponentKinds` registry.
    pub fn new() -> Self {
        Self {
            current_net_id: 0,
            kind_bit_width: 0,
            kind_map: HashMap::new(),
            net_id_map: HashMap::new(),
            user_dependent: HashSet::new(),
            facts: HashMap::new(),
        }
    }

    /// Registers replicated component type `C`, assigning it the next sequential net-ID.
    pub fn add_component<C: Replicate>(&mut self) {
        let component_kind = ComponentKind::of::<C>();

        let net_id = self.current_net_id;
        // Pre-2026-05-05 there was a `net_id < 64` cap here because the
        // per-user `DirtyQueue` stored dirty bits in a single `u64`
        // per entity. The queue is now flat-strided over multiple
        // `u64`s sized to `ceil(kind_count / 64)`, so there's no
        // longer a 64-kind ceiling. The wire-format kind tag is a
        // `u16` NetId (cap 65,535) — that's the real ceiling and well
        // beyond any realistic protocol size.

        // Enforce the CachedComponentUpdate ceiling. `#[derive(Replicate)]`
        // computes a precise bound whenever every property's inner type is
        // `ConstBitLength`, and returns UNBOUNDED_BIT_LENGTH otherwise (a `Vec`,
        // a `String`, a variable-length number, or an `EntityProperty`, which
        // encodes per-connection). The sentinel SKIPS this assert rather than
        // failing it — an unbounded component is admitted here and instead trips
        // the `capture()` expect in `world_writer.rs` the first time it
        // serializes past the cache. Prefer const-bounded fields for anything
        // large enough to care, so the failure lands at registration.
        let max_bits = C::max_bit_length();
        if max_bits != UNBOUNDED_BIT_LENGTH {
            assert!(
                max_bits <= CACHED_UPDATE_BITS,
                "Component {} serializes to {} bits, exceeding the {}-bit \
                 CachedComponentUpdate ceiling. Slim the component before registering.",
                std::any::type_name::<C>(),
                max_bits,
                CACHED_UPDATE_BITS
            );
        }
        if C::has_entity_properties() {
            self.user_dependent.insert(component_kind);
        }

        let builder = C::create_builder();
        let immutable = builder.is_immutable();
        let property_labels: Vec<String> = C::replicated_property_labels()
            .into_iter()
            .map(str::to_string)
            .collect();
        let mask_indices = C::property_mask_indices();
        let entity_property_labels: Vec<String> = C::entity_property_labels()
            .into_iter()
            .map(str::to_string)
            .collect();
        // Labels and mask indices are parallel lists from the same derive
        // walk; every entity label must name a wired property.
        debug_assert_eq!(
            property_labels.len(),
            mask_indices.len(),
            "Component {} has {} property labels but {} mask indices",
            std::any::type_name::<C>(),
            property_labels.len(),
            mask_indices.len(),
        );
        debug_assert!(
            entity_property_labels
                .iter()
                .all(|label| property_labels.contains(label)),
            "Component {} has entity labels outside its wired properties",
            std::any::type_name::<C>(),
        );
        self.facts.insert(
            component_kind,
            ComponentFacts {
                name: C::protocol_name().to_string(),
                descriptor: C::wire_schema(),
                immutable,
                property_labels: property_labels.clone(),
                mask_indices,
                mask_size_bytes: C::mask_size_bytes(),
                has_entity_properties: C::has_entity_properties(),
                entity_property_labels,
                authority_delegable: !immutable && !property_labels.is_empty(),
            },
        );

        self.kind_map.insert(
            component_kind,
            (net_id, builder, C::protocol_name().to_string()),
        );
        self.net_id_map.insert(net_id, component_kind);
        self.current_net_id += 1;
        self.kind_bit_width = bit_width_for_kind_count(self.current_net_id);
    }

    /// Returns every registered component's structural facts in **wire
    /// net-ID order**, as `(net_id, facts)`.
    ///
    /// Same traversal contract as the message registry's descriptor entries:
    /// net-IDs are dense registration ordinals, so the walk covers the whole
    /// net-ID space and `HashMap` iteration order never leaks into the
    /// result.
    pub fn schema_fact_entries(&self) -> Vec<(NetId, ComponentFacts)> {
        let mut output = Vec::with_capacity(self.current_net_id as usize);
        for net_id in 0..self.current_net_id {
            let kind = self
                .net_id_map
                .get(&net_id)
                .expect("ComponentKinds net-ID space must be dense");
            let facts = self
                .facts
                .get(kind)
                .expect("every registered ComponentKind must have facts");
            output.push((net_id, facts.clone()));
        }
        output
    }

    /// Resolves a registered `ComponentKind` to its wire net-ID, or `None`
    /// if the kind was never registered here. Used to resolve resource
    /// membership (which is kind-keyed) to the net-IDs the fingerprint
    /// walks.
    /// Returns `true` if this component kind has `EntityProperty` fields —
    /// its serialized bytes differ per connection and cannot use the shared cache.
    pub fn is_user_dependent(&self, kind: &ComponentKind) -> bool {
        self.user_dependent.contains(kind)
    }

    /// Returns the `ComponentKind` for the given `net_id`, or `None` if not registered.
    /// Provides O(1) inverse lookup from NetId to ComponentKind.
    pub fn kind_for_net_id(&self, net_id: u16) -> Option<ComponentKind> {
        self.net_id_map.get(&net_id).copied()
    }

    /// Number of component kinds currently registered. Used at
    /// `UserDiffHandler` construction to size the per-user `DirtyQueue`'s
    /// stride (= `ceil(kind_count / 64)` AtomicU64 words per entity).
    pub fn kind_count(&self) -> u16 {
        self.current_net_id
    }

    /// Reads a component kind tag then deserializes and returns the component from `reader`.
    pub fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<Box<dyn Replicate>, SerdeErr> {
        let component_kind: ComponentKind = ComponentKind::de(self, reader)?;
        self.kind_to_builder(&component_kind)
            .read(reader, converter)
    }

    /// Reads a component kind tag then deserializes an initial-create update payload from `reader`.
    pub fn read_create_update(
        &self,
        reader: &mut BitReader,
    ) -> Result<PendingComponentUpdate, SerdeErr> {
        let component_kind: ComponentKind = ComponentKind::de(self, reader)?;
        self.kind_to_builder(&component_kind)
            .read_create_update(reader)
    }

    /// Splits a full-component update into a waiting portion and a ready-to-apply portion.
    pub fn split_update(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        component_kind: &ComponentKind,
        update: PendingComponentUpdate,
    ) -> SplitUpdateResult {
        self.kind_to_builder(component_kind)
            .split_update(converter, update)
    }

    /// Returns the protocol name for `component_kind`. Panics if not registered.
    pub fn kind_to_name(&self, component_kind: &ComponentKind) -> String {
        self.kind_map
            .get(component_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_component()` function!",
            )
            .2
            .clone()
    }

    /// Resolves a net-ID read from the wire into a registered `ComponentKind`.
    ///
    /// The net-ID comes from a remote peer, so an unregistered value is a
    /// malformed packet rather than a local programming error: return an error
    /// and let the caller drop the packet.
    fn net_id_to_kind(&self, net_id: &NetId) -> Result<ComponentKind, SerdeErr> {
        self.net_id_map.get(net_id).copied().ok_or(SerdeErr)
    }

    fn kind_to_net_id(&self, component_kind: &ComponentKind) -> NetId {
        self.kind_map
            .get(component_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_component()` function!",
            )
            .0
    }

    /// Public accessor for a kind's NetId (== bit position in the
    /// `DirtyQueue` u64 mask, max 64). Returns `None` for unregistered
    /// kinds.
    pub fn net_id_of(&self, component_kind: &ComponentKind) -> Option<u16> {
        self.kind_map
            .get(component_kind)
            .map(|(net_id, _, _)| *net_id)
    }

    fn kind_to_builder(&self, component_kind: &ComponentKind) -> &dyn ReplicateBuilder {
        self.kind_map
            .get(component_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_component()` function!",
            )
            .1
            .as_ref()
    }

    /// Returns `true` if the given kind was registered as an immutable component.
    pub fn kind_is_immutable(&self, component_kind: &ComponentKind) -> bool {
        self.kind_map
            .get(component_kind)
            .map(|(_, builder, _)| builder.is_immutable())
            .unwrap_or(false)
    }

    /// Returns every registered component in **wire net-ID order**, as
    /// `(net_id, protocol_name)`.
    ///
    /// The protocol fingerprint is built from this rather than from
    /// [`all_names`](Self::all_names): net-IDs are registration ordinals that
    /// travel on the wire, so hashing sorted names would leave a reordering
    /// that renumbers every component undetectable. Walking the net-ID space
    /// also keeps `HashMap` iteration order out of the result, which is what
    /// makes the fingerprint reproducible across processes.
    ///
    /// Net-IDs are dense by construction, so a gap is a broken invariant.
    pub fn schema_entries(&self) -> Vec<(NetId, String)> {
        let mut output = Vec::with_capacity(self.current_net_id as usize);
        for net_id in 0..self.current_net_id {
            let kind = self
                .net_id_map
                .get(&net_id)
                .expect("ComponentKinds net-ID space must be dense");
            let (_, _, name) = self
                .kind_map
                .get(kind)
                .expect("every registered ComponentKind must have a name");
            output.push((net_id, name.clone()));
        }
        output
    }

    /// Returns a sorted list of all registered component protocol names.
    pub fn all_names(&self) -> Vec<String> {
        let mut output = Vec::new();
        for (_, _, name) in self.kind_map.values() {
            output.push(name.clone());
        }
        output.sort();
        output
    }
}

#[cfg(test)]
mod tests {
    use naia_serde::BitReader;

    use crate::{ComponentKind, ComponentKinds};

    /// A net_id with no registered `Component` behind it is something a remote
    /// peer can put on the wire, so decoding it must return an error the caller
    /// can drop the packet on -- not panic and take the whole process down.
    #[test]
    fn unregistered_net_id_errors_instead_of_panicking() {
        let kinds = ComponentKinds::new();
        let bytes = [0u8; 4];
        let mut reader = BitReader::new(&bytes);
        assert!(ComponentKind::de(&kinds, &mut reader).is_err());
    }
}
