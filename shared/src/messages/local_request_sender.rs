use std::{time::Duration, collections::HashMap};

use naia_derive::MessageRequest;
use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::{types::GlobalRequestId, KeyGenerator, LocalEntityAndGlobalEntityConverterMut, MessageContainer, MessageKind, MessageKinds};

pub struct LocalRequestSender {
    channels: HashMap<MessageKind, LocalRequestSenderChannel>,
}

impl LocalRequestSender {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    pub(crate) fn process_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        global_request_id: GlobalRequestId,
        request: MessageContainer
    ) -> MessageContainer {
        let key = request.kind();
        if !self.channels.contains_key(&key) {
            self.channels.insert(key.clone(), LocalRequestSenderChannel::new());
        }
        let channel = self.channels.get_mut(&key).unwrap();
        channel.process_request(message_kinds, converter, global_request_id, request)
    }
}

pub struct LocalRequestSenderChannel {
    local_key_generator: KeyGenerator<LocalRequestId>,
    local_to_global_ids: HashMap<LocalRequestId, GlobalRequestId>,
}

impl LocalRequestSenderChannel {
    pub fn new() -> Self {
        Self {
            local_key_generator: KeyGenerator::new(Duration::from_secs(60)),
            local_to_global_ids: HashMap::new(),
        }
    }

    pub(crate) fn process_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        global_request_id: GlobalRequestId,
        request: MessageContainer
    ) -> MessageContainer {

        let request_key = self.local_key_generator.generate();
        self.local_to_global_ids.insert(request_key, global_request_id);

        let mut writer = BitWriter::with_max_capacity();
        request.write(message_kinds, &mut writer, converter);
        let request_bytes = writer.to_bytes();
        let request_message = RequestMessage::new(request_key, request_bytes);
        MessageContainer::from_write(Box::new(request_message), converter)
    }
}

#[derive(MessageRequest)]
pub struct RequestMessage {
    request_id: LocalRequestId,
    bytes: Box<[u8]>,
}

impl RequestMessage {
    pub fn new(request_id: LocalRequestId, bytes: Box<[u8]>) -> Self {
        Self {
            request_id,
            bytes,
        }
    }

    pub(crate) fn to_id_and_bytes(self) -> (LocalRequestId, Box<[u8]>) {
        (self.request_id, self.bytes)
    }
}

pub type LocalResponseId = LocalRequestId;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct LocalRequestId {
    id: u8,
}

impl LocalRequestId {
    pub(crate) fn new(id: u8) -> Self {
        Self { id }
    }
}

impl From<u16> for LocalRequestId {
    fn from(id: u16) -> Self {
        Self { id: id as u8 }
    }
}

impl Into<u16> for LocalRequestId {
    fn into(self) -> u16 {
        self.id as u16
    }
}

impl Serde for LocalRequestId {
    fn ser(&self, writer: &mut dyn BitWrite) {
        self.id.ser(writer)
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let id = u8::de(reader)?;
        Ok(Self { id })
    }

    fn bit_length(&self) -> u32 {
        self.id.bit_length()
    }
}