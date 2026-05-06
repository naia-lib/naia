use std::{any::TypeId, collections::HashMap};

use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

use crate::{LocalEntityAndGlobalEntityConverter, Message, MessageBuilder, MessageContainer};

type NetId = u16;

/// Wire encoding for `MessageKind` NetIds: a fixed-width raw bit field
/// whose width is `ceil(log2(N))` for the protocol's registered message
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

/// MessageKind - should be one unique value for each type of Message
#[derive(Eq, Hash, Copy, Clone, PartialEq, Debug)]
pub struct MessageKind {
    type_id: TypeId,
}

impl MessageKind {
    pub fn of<M: Message>() -> Self {
        Self {
            type_id: TypeId::of::<M>(),
        }
    }

    pub fn ser(&self, message_kinds: &MessageKinds, writer: &mut dyn BitWrite) {
        let net_id = message_kinds.kind_to_net_id(self);
        let bits = message_kinds.kind_bit_width;
        for i in 0..bits {
            writer.write_bit((net_id >> i) & 1 != 0);
        }
    }

    pub fn de(message_kinds: &MessageKinds, reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let bits = message_kinds.kind_bit_width;
        let mut net_id: NetId = 0;
        for i in 0..bits {
            if bool::de(reader)? {
                net_id |= 1 << i;
            }
        }
        Ok(message_kinds.net_id_to_kind(&net_id))
    }
}

// MessageKinds
pub struct MessageKinds {
    current_net_id: NetId,
    /// Number of bits needed to encode any registered NetId — recomputed
    /// on every `add_message`. Read directly by `MessageKind::ser`/`de`
    /// on the hot path.
    kind_bit_width: u8,
    kind_map: HashMap<MessageKind, (NetId, Box<dyn MessageBuilder>, String)>,
    net_id_map: HashMap<NetId, MessageKind>,
}

impl Clone for MessageKinds {
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

impl Default for MessageKinds {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageKinds {
    pub fn new() -> Self {
        Self {
            current_net_id: 0,
            kind_bit_width: 0,
            kind_map: HashMap::new(),
            net_id_map: HashMap::new(),
        }
    }

    pub fn add_message<M: Message>(&mut self) {
        let message_kind = MessageKind::of::<M>();

        let net_id = self.current_net_id;
        self.kind_map.insert(
            message_kind,
            (net_id, M::create_builder(), M::protocol_name().to_string()),
        );
        self.net_id_map.insert(net_id, message_kind);
        self.current_net_id += 1;
        self.kind_bit_width = bit_width_for_kind_count(self.current_net_id);
        //TODO: check for current_id overflow?
    }

    /// Bit width of every encoded `MessageKind` in this registry. Used by
    /// derived `Message::bit_length` impls to size the kind-tag prefix.
    pub fn kind_bit_length(&self) -> u32 {
        self.kind_bit_width as u32
    }

    pub fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<MessageContainer, SerdeErr> {
        let message_kind: MessageKind = MessageKind::de(self, reader)?;
        self.kind_to_builder(&message_kind).read(reader, converter)
    }

    fn net_id_to_kind(&self, net_id: &NetId) -> MessageKind {
        *self.net_id_map.get(net_id).expect(
            "Must properly initialize Message with Protocol via `add_message()` function!",
        )
    }

    fn kind_to_net_id(&self, message_kind: &MessageKind) -> NetId {
        self
            .kind_map
            .get(message_kind)
            .expect("Must properly initialize Message with Protocol via `add_message()` function!")
            .0
    }

    fn kind_to_builder(&self, message_kind: &MessageKind) -> &Box<dyn MessageBuilder> {
        &self
            .kind_map
            .get(message_kind)
            .expect("Must properly initialize Message with Protocol via `add_message()` function!")
            .1
    }

    pub fn all_names(&self) -> Vec<String> {
        let mut output = Vec::new();
        for (_, _, name) in self.kind_map.values() {
            output.push(name.clone());
        }
        output.sort();
        output
    }
}
