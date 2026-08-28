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
        channel_kinds.net_id_to_kind(&net_id)
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

    /// Resolves a net-ID read from the wire into a registered `ChannelKind`.
    ///
    /// The net-ID comes from a remote peer, so an unregistered value is a
    /// malformed packet rather than a local programming error: return an error
    /// and let the caller drop the packet.
    fn net_id_to_kind(&self, net_id: &NetId) -> Result<ChannelKind, SerdeErr> {
        self.net_id_map.get(net_id).copied().ok_or(SerdeErr)
    }

    fn kind_to_net_id(&self, channel_kind: &ChannelKind) -> NetId {
        self.kind_map
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

#[cfg(test)]
mod tests {
    use naia_serde::{BitReader, BitWrite, BitWriter};

    use crate::{
        Channel, ChannelDirection, ChannelKind, ChannelKinds, ChannelMode, ChannelSettings, Named,
        ReliableSettings,
    };

    macro_rules! test_channel {
        ($name:ident) => {
            struct $name;
            impl Named for $name {
                fn name(&self) -> String {
                    stringify!($name).to_string()
                }
                fn protocol_name() -> &'static str {
                    stringify!($name)
                }
            }
            impl Channel for $name {}
        };
    }

    test_channel!(ChannelA);
    test_channel!(ChannelB);
    test_channel!(ChannelC);

    fn kinds() -> ChannelKinds {
        let mut kinds = ChannelKinds::new();
        let settings = ChannelSettings::new(
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ChannelDirection::Bidirectional,
        );
        kinds.add_channel::<ChannelA>(settings.clone());
        kinds.add_channel::<ChannelB>(settings.clone());
        kinds.add_channel::<ChannelC>(settings);
        kinds
    }

    fn read_net_id(kinds: &ChannelKinds, net_id: u16) -> Result<ChannelKind, naia_serde::SerdeErr> {
        let mut writer = BitWriter::new();
        for i in 0..kinds.kind_bit_width {
            writer.write_bit((net_id >> i) & 1 != 0);
        }
        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        ChannelKind::de(kinds, &mut reader)
    }

    #[test]
    fn registered_net_id_decodes() {
        let kinds = kinds();
        assert_eq!(
            read_net_id(&kinds, 1).unwrap(),
            ChannelKind::of::<ChannelB>()
        );
    }

    /// Three channels round up to a 2-bit tag, so net_ids 3 is encodable but
    /// unregistered. A remote peer can put it on the wire, so decoding it must
    /// return an error the caller can drop the packet on -- not panic and take
    /// the whole process down.
    #[test]
    fn unregistered_net_id_errors_instead_of_panicking() {
        let kinds = kinds();
        assert!(read_net_id(&kinds, 3).is_err());
    }
}
