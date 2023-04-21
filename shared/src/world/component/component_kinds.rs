use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr};

use crate::{
    ComponentUpdate, LocalEntity, LocalEntityAndGlobalEntityConverter, Replicate, ReplicateBuilder,
};

type NetId = u16;

/// ComponentKind - should be one unique value for each type of Component
#[derive(Eq, Hash, Copy, Clone, PartialEq)]
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
        component_kinds.kind_to_net_id(self).ser(writer);
    }

    pub fn de(component_kinds: &ComponentKinds, reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let net_id: NetId = NetId::de(reader)?;
        Ok(component_kinds.net_id_to_kind(&net_id))
    }
}

impl ConstBitLength for ComponentKind {
    fn const_bit_length() -> u32 {
        <NetId as ConstBitLength>::const_bit_length()
    }
}

/// A map to hold all component types
pub struct ComponentKinds {
    current_net_id: NetId,
    kind_map: HashMap<ComponentKind, (NetId, Box<dyn ReplicateBuilder>)>,
    net_id_map: HashMap<NetId, ComponentKind>,
}

impl ComponentKinds {
    pub fn new() -> Self {
        Self {
            current_net_id: 0,
            kind_map: HashMap::new(),
            net_id_map: HashMap::new(),
        }
    }

    pub fn add_component<C: Replicate>(&mut self) {
        let component_kind = ComponentKind::of::<C>();

        let net_id = self.current_net_id;
        self.kind_map
            .insert(component_kind, (net_id, C::create_builder()));
        self.net_id_map.insert(net_id, component_kind);
        self.current_net_id += 1;
        //TODO: check for current_id overflow?
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
            Option<(HashSet<LocalEntity>, ComponentUpdate)>,
            Option<ComponentUpdate>,
        ),
        SerdeErr,
    > {
        return self
            .kind_to_builder(component_kind)
            .split_update(converter, update);
    }

    pub fn kind_to_name(&self, component_kind: &ComponentKind) -> String {
        return self.kind_to_builder(component_kind).name();
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

    fn kind_to_builder(&self, component_kind: &ComponentKind) -> &Box<dyn ReplicateBuilder> {
        return &self
            .kind_map
            .get(component_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_component()` function!",
            )
            .1;
    }
}
