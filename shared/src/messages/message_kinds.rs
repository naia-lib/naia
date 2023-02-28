use std::{any::TypeId, collections::HashMap};

use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr};

use crate::{Message, MessageBuilder, MessageContainer, NetEntityHandleConverter};

type NetId = u16;

/// MessageKind - should be one unique value for each type of Message
#[derive(Eq, Hash, Copy, Clone, PartialEq)]
pub struct MessageKind {
    type_id: TypeId,
}

// impl From<TypeId> for MessageKind {
//     fn from(type_id: TypeId) -> Self {
//         Self {
//             type_id
//         }
//     }
// }

impl MessageKind {
    pub fn of<M: Message>() -> Self {
        Self {
            type_id: TypeId::of::<M>(),
        }
    }

    pub fn ser(&self, message_kinds: &MessageKinds, writer: &mut dyn BitWrite) {
        message_kinds.kind_to_net_id(self).ser(writer);
    }

    pub fn de(message_kinds: &MessageKinds, reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let net_id: NetId = NetId::de(reader)?;
        Ok(message_kinds.net_id_to_kind(&net_id))
    }
}

impl ConstBitLength for MessageKind {
    fn const_bit_length() -> u32 {
        <NetId as ConstBitLength>::const_bit_length()
    }
}

// MessageKinds
pub struct MessageKinds {
    current_net_id: NetId,
    kind_map: HashMap<MessageKind, (NetId, Box<dyn MessageBuilder>)>,
    net_id_map: HashMap<NetId, MessageKind>,
}

impl MessageKinds {
    pub fn new() -> Self {
        Self {
            current_net_id: 0,
            kind_map: HashMap::new(),
            net_id_map: HashMap::new(),
        }
    }

    pub fn add_message<M: Message>(&mut self) {
        let message_kind = MessageKind::of::<M>();

        let net_id = self.current_net_id;
        self.kind_map
            .insert(message_kind, (net_id, M::create_builder()));
        self.net_id_map.insert(net_id, message_kind);
        self.current_net_id += 1;
        //TODO: check for current_id overflow?
    }

    pub fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<MessageContainer, SerdeErr> {
        let message_kind: MessageKind = MessageKind::de(self, reader)?;
        return self.kind_to_builder(&message_kind).read(reader, converter);
    }

    fn net_id_to_kind(&self, net_id: &NetId) -> MessageKind {
        return *self.net_id_map.get(net_id).expect(
            "Must properly initialize Message with Protocol via `add_message()` function!",
        );
    }

    fn kind_to_net_id(&self, message_kind: &MessageKind) -> NetId {
        return self
            .kind_map
            .get(message_kind)
            .expect("Must properly initialize Message with Protocol via `add_message()` function!")
            .0;
    }

    fn kind_to_builder(&self, message_kind: &MessageKind) -> &Box<dyn MessageBuilder> {
        return &self
            .kind_map
            .get(&message_kind)
            .expect("Must properly initialize Message with Protocol via `add_message()` function!")
            .1;
    }
}
