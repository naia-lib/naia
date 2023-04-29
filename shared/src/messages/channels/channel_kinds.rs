use std::{any::TypeId, collections::HashMap};

use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr};

use crate::messages::channels::channel::{Channel, ChannelSettings};

type NetId = u16;

/// ChannelKind - should be one unique value for each type of Channel
#[derive(Eq, Hash, Copy, Clone, PartialEq)]
pub struct ChannelKind {
    type_id: TypeId,
}

// impl From<TypeId> for ChannelKind {
//     fn from(type_id: TypeId) -> Self {
//         Self {
//             type_id
//         }
//     }
// }

impl ChannelKind {
    pub fn of<C: Channel>() -> Self {
        Self {
            type_id: TypeId::of::<C>(),
        }
    }

    pub fn ser(&self, channel_kinds: &ChannelKinds, writer: &mut dyn BitWrite) {
        channel_kinds.kind_to_net_id(self).ser(writer);
    }

    pub fn de(channel_kinds: &ChannelKinds, reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let net_id: NetId = NetId::de(reader)?;
        Ok(channel_kinds.net_id_to_kind(&net_id))
    }
}

impl ConstBitLength for ChannelKind {
    fn const_bit_length() -> u32 {
        <NetId as ConstBitLength>::const_bit_length()
    }
}

// ChannelKinds
pub struct ChannelKinds {
    current_net_id: NetId,
    kind_map: HashMap<ChannelKind, (NetId, ChannelSettings)>,
    net_id_map: HashMap<NetId, ChannelKind>,
}

impl ChannelKinds {
    pub fn new() -> Self {
        Self {
            current_net_id: 0,
            kind_map: HashMap::new(),
            net_id_map: HashMap::new(),
        }
    }

    pub fn add_channel<C: Channel>(&mut self, settings: ChannelSettings) {
        let channel_kind = ChannelKind::of::<C>();
        let net_id = self.current_net_id;
        self.kind_map.insert(channel_kind, (net_id, settings));
        self.net_id_map.insert(net_id, channel_kind);
        self.current_net_id += 1;
        //TODO: check for current_id overflow?
    }

    pub fn channels(&self) -> Vec<(ChannelKind, ChannelSettings)> {
        // TODO: is there a better way to do this without copying + cloning?
        // How to return a reference here (behind a Mutex ..)
        let mut output = Vec::new();
        for (kind, (_, settings)) in &self.kind_map {
            output.push((*kind, settings.clone()));
        }
        output
    }

    pub fn channel(&self, kind: &ChannelKind) -> ChannelSettings {
        let (_, settings) = self.kind_map.get(kind).expect("could not find ChannelKind for given Channel. Make sure Channel struct has `#[derive(Channel)]` on it!");
        settings.clone()
    }

    fn net_id_to_kind(&self, net_id: &NetId) -> ChannelKind {
        return *self.net_id_map.get(net_id).expect(
            "Must properly initialize Channel with Protocol via `add_channel()` function!",
        );
    }

    fn kind_to_net_id(&self, channel_kind: &ChannelKind) -> NetId {
        return self
            .kind_map
            .get(channel_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_channel()` function!",
            )
            .0;
    }
}
