use std::{any::TypeId, collections::HashMap};

use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

use crate::{
    ComponentFieldUpdate, ComponentUpdate, LocalEntityAndGlobalEntityConverter, RemoteEntity,
    Replicate, ReplicateBuilder,
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
impl Into<TypeId> for ComponentKind {
    fn into(self) -> TypeId {
        self.type_id
    }
}

impl ComponentKind {
    pub fn of<C: Replicate>() -> Self {
        Self {
            type_id: TypeId::of::<C>(),
        }
    }

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
}

impl Clone for ComponentKinds {
    fn clone(&self) -> Self {
        let current_net_id = self.current_net_id;
        let kind_bit_width = self.kind_bit_width;
        let net_id_map = self.net_id_map.clone();

        let mut kind_map = HashMap::new();
        for (key, value) in self.kind_map.iter() {
            kind_map.insert(*key, (value.0, value.1.box_clone(), value.2.clone()));
        }

        Self {
            current_net_id,
            kind_bit_width,
            kind_map,
            net_id_map,
        }
    }
}

impl ComponentKinds {
    pub fn new() -> Self {
        Self {
            current_net_id: 0,
            kind_bit_width: 0,
            kind_map: HashMap::new(),
            net_id_map: HashMap::new(),
        }
    }

    pub fn add_component<C: Replicate>(&mut self) {
        let component_kind = ComponentKind::of::<C>();

        let net_id = self.current_net_id;
        assert!(
            net_id < 64,
            "DirtySet bitset supports max 64 component kinds; protocol has {}. \
             Extend `DirtyQueue::dirty_bits` to two u64s per entity if you need more.",
            net_id + 1,
        );
        self.kind_map.insert(
            component_kind,
            (net_id, C::create_builder(), C::protocol_name().to_string()),
        );
        self.net_id_map.insert(net_id, component_kind);
        self.current_net_id += 1;
        self.kind_bit_width = bit_width_for_kind_count(self.current_net_id);
    }

    pub fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<Box<dyn Replicate>, SerdeErr> {
        let component_kind: ComponentKind = ComponentKind::de(self, reader)?;
        return self
            .kind_to_builder(&component_kind)
            .read(reader, converter);
    }

    pub fn read_create_update(&self, reader: &mut BitReader) -> Result<ComponentUpdate, SerdeErr> {
        let component_kind: ComponentKind = ComponentKind::de(self, reader)?;
        return self
            .kind_to_builder(&component_kind)
            .read_create_update(reader);
    }

    pub fn split_update(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        component_kind: &ComponentKind,
        update: ComponentUpdate,
    ) -> Result<
        (
            Option<Vec<(RemoteEntity, ComponentFieldUpdate)>>,
            Option<ComponentUpdate>,
        ),
        SerdeErr,
    > {
        return self
            .kind_to_builder(component_kind)
            .split_update(converter, update);
    }

    pub fn kind_to_name(&self, component_kind: &ComponentKind) -> String {
        return self
            .kind_map
            .get(component_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_component()` function!",
            )
            .2
            .clone();
    }

    fn net_id_to_kind(&self, net_id: &NetId) -> ComponentKind {
        return *self.net_id_map.get(net_id).expect(
            "Must properly initialize Component with Protocol via `add_component()` function!",
        );
    }

    fn kind_to_net_id(&self, component_kind: &ComponentKind) -> NetId {
        return self
            .kind_map
            .get(component_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_component()` function!",
            )
            .0;
    }

    /// Public accessor for a kind's NetId (== bit position in the
    /// `DirtyQueue` u64 mask, max 64). Returns `None` for unregistered
    /// kinds.
    pub fn net_id_of(&self, component_kind: &ComponentKind) -> Option<u16> {
        self.kind_map.get(component_kind).map(|(net_id, _, _)| *net_id)
    }

    fn kind_to_builder(&self, component_kind: &ComponentKind) -> &Box<dyn ReplicateBuilder> {
        return &self
            .kind_map
            .get(component_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_component()` function!",
            )
            .1;
    }

    pub fn kind_is_immutable(&self, component_kind: &ComponentKind) -> bool {
        self.kind_map
            .get(component_kind)
            .map(|(_, builder, _)| builder.is_immutable())
            .unwrap_or(false)
    }

    pub fn all_names(&self) -> Vec<String> {
        let mut output = Vec::new();
        for (_, (_, _, name)) in &self.kind_map {
            output.push(name.clone());
        }
        output.sort();
        output
    }
}
