use std::{any::TypeId, collections::HashMap};

use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

use crate::messages::channels::channel::{Channel, ChannelSettings};

type NetId = u16;

/// Wire encoding for `ChannelKind` NetIds: a fixed-width raw bit field
/// whose width is `ceil(log2(N))` for the protocol's registered channel
/// count. Both ends share registration order, so both compute the same
/// width. See `world::component::component_kinds` for the matching
/// rationale on the component side — same logic, same shape.
fn bit_width_for_kind_count(count: NetId) -> u8 {
    if count < 2 {
        0
    } else {
        (count as u32).next_power_of_two().trailing_zeros() as u8
    }
}

/// ChannelKind - should be one unique value for each type of Channel
#[derive(Eq, Hash, Copy, Clone, PartialEq, Debug)]
pub struct ChannelKind {
    type_id: TypeId,
}

impl ChannelKind {
    /// Returns the `ChannelKind` corresponding to the type `C`.
    pub fn of<C: Channel>() -> Self {
        Self {
            type_id: TypeId::of::<C>(),
        }
    }

    /// Serializes this kind's compact net-ID into `writer` using the bit-width registered in `channel_kinds`.
    pub fn ser(&self, channel_kinds: &ChannelKinds, writer: &mut dyn BitWrite) {
        let net_id = channel_kinds.kind_to_net_id(self);
        let bits = channel_kinds.kind_bit_width;
        for i in 0..bits {
            writer.write_bit((net_id >> i) & 1 != 0);
        }
    }

    /// Deserializes a `ChannelKind` from `reader` using the bit-width registered in `channel_kinds`.
    pub fn de(channel_kinds: &ChannelKinds, reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let bits = channel_kinds.kind_bit_width;
        let mut net_id: NetId = 0;
        for i in 0..bits {
            if bool::de(reader)? {
                net_id |= 1 << i;
            }
        }
        Ok(channel_kinds.net_id_to_kind(&net_id))
    }
}

/// Registry mapping `Channel` types to compact wire net-IDs and their `ChannelSettings`.
#[derive(Clone)]
pub struct ChannelKinds {
    current_net_id: NetId,
    /// Number of bits needed to encode any registered NetId — recomputed
    /// on every `add_channel`. Read directly by `ChannelKind::ser`/`de`
    /// on the hot path.
    kind_bit_width: u8,
    kind_map: HashMap<ChannelKind, (NetId, ChannelSettings, String)>,
    net_id_map: HashMap<NetId, ChannelKind>,
}

impl Default for ChannelKinds {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelKinds {
    /// Creates an empty `ChannelKinds` registry.
    pub fn new() -> Self {
        Self {
            current_net_id: 0,
            kind_bit_width: 0,
            kind_map: HashMap::new(),
            net_id_map: HashMap::new(),
        }
    }

    /// Registers channel type `C` with the given settings, assigning it the next sequential net-ID.
    pub fn add_channel<C: Channel>(&mut self, settings: ChannelSettings) {
        let channel_kind = ChannelKind::of::<C>();
        //info!("ChannelKinds adding channel: {:?}", channel_kind);
        let net_id = self.current_net_id;
        self.kind_map.insert(
            channel_kind,
            (net_id, settings, C::protocol_name().to_string()),
        );
        self.net_id_map.insert(net_id, channel_kind);
        debug_assert!(
            self.current_net_id < NetId::MAX,
            "ChannelKinds NetId overflow — too many channels registered (max {})",
            NetId::MAX
        );
        self.current_net_id += 1;
        self.kind_bit_width = bit_width_for_kind_count(self.current_net_id);
    }

    /// Returns all registered `(ChannelKind, ChannelSettings)` pairs.
    pub fn channels(&self) -> Vec<(ChannelKind, ChannelSettings)> {
        // TODO: is there a better way to do this without copying + cloning?
        // How to return a reference here (behind a Mutex ..)
        let mut output = Vec::new();
        for (kind, (_, settings, _)) in &self.kind_map {
            output.push((*kind, settings.clone()));
        }
        output
    }

    /// Returns the `ChannelSettings` for the given kind. Panics if the kind was not registered.
    pub fn channel(&self, kind: &ChannelKind) -> ChannelSettings {
        let (_, settings, _) = self.kind_map.get(kind).expect("could not find ChannelKind for given Channel. Make sure Channel struct has `#[derive(Channel)]` on it!");
        settings.clone()
    }

    fn net_id_to_kind(&self, net_id: &NetId) -> ChannelKind {
        *self.net_id_map.get(net_id).expect(
            "Must properly initialize Channel with Protocol via `add_channel()` function!",
        )
    }

    fn kind_to_net_id(&self, channel_kind: &ChannelKind) -> NetId {
        self
            .kind_map
            .get(channel_kind)
            .expect(
                "Must properly initialize Component with Protocol via `add_channel()` function!",
            )
            .0
    }

    /// Returns a sorted list of all registered channel protocol names.
    pub fn all_names(&self) -> Vec<String> {
        let mut output = Vec::new();
        for (_, _, name) in self.kind_map.values() {
            output.push(name.clone());
        }
        output.sort();
        output
    }

    /// Returns the protocol name for `kind`, or `None` if not registered.
    pub fn channel_name(&self, kind: &ChannelKind) -> Option<&str> {
        self.kind_map.get(kind).map(|(_, _, name)| name.as_str())
    }

    /// Returns all `(ChannelKind, protocol_name)` pairs registered in this registry.
    pub fn channel_names(&self) -> Vec<(ChannelKind, String)> {
        self.kind_map
            .iter()
            .map(|(kind, (_, _, name))| (*kind, name.clone()))
            .collect()
    }
}
