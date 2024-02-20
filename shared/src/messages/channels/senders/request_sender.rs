use std::{collections::HashMap, time::Duration};

use naia_derive::MessageRequest;
use naia_serde::{BitWriter, SerdeInternal};

use crate::messages::request::GlobalRequestId;
use crate::{KeyGenerator, LocalEntityAndGlobalEntityConverterMut, MessageContainer, MessageKinds};

pub struct RequestSender {
    local_key_generator: KeyGenerator<LocalRequestId>,
    local_to_global_ids: HashMap<LocalRequestId, GlobalRequestId>,
}

impl RequestSender {
    pub fn new() -> Self {
        Self {
            local_key_generator: KeyGenerator::new(Duration::from_secs(60)),
            local_to_global_ids: HashMap::new(),
        }
    }

    pub(crate) fn process_outgoing_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        global_request_id: GlobalRequestId,
        request: MessageContainer,
    ) -> MessageContainer {
        let local_request_id = self.local_key_generator.generate();
        self.local_to_global_ids
            .insert(local_request_id, global_request_id);

        let mut writer = BitWriter::with_max_capacity();
        request.write(message_kinds, &mut writer, converter);
        let request_bytes = writer.to_bytes();
        let request_message = RequestOrResponse::request(local_request_id, request_bytes);
        MessageContainer::from_write(Box::new(request_message), converter)
    }

    pub(crate) fn process_outgoing_response(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        local_response_id: LocalResponseId,
        response: MessageContainer,
    ) -> MessageContainer {
        let mut writer = BitWriter::with_max_capacity();
        response.write(message_kinds, &mut writer, converter);
        let response_bytes = writer.to_bytes();
        let response_message = RequestOrResponse::response(local_response_id, response_bytes);
        MessageContainer::from_write(Box::new(response_message), converter)
    }

    pub(crate) fn process_incoming_response(
        &mut self,
        local_request_id: &LocalRequestId,
    ) -> Option<GlobalRequestId> {
        self.local_key_generator.recycle_key(local_request_id);
        self.local_to_global_ids.remove(local_request_id)
    }
}

#[derive(MessageRequest)]
pub struct RequestOrResponse {
    id: LocalRequestOrResponseId,
    bytes: Box<[u8]>,
}

impl RequestOrResponse {
    pub fn request(id: LocalRequestId, bytes: Box<[u8]>) -> Self {
        Self {
            id: id.to_req_res_id(),
            bytes,
        }
    }

    pub fn response(id: LocalResponseId, bytes: Box<[u8]>) -> Self {
        Self {
            id: id.to_req_res_id(),
            bytes,
        }
    }

    pub(crate) fn to_id_and_bytes(self) -> (LocalRequestOrResponseId, Box<[u8]>) {
        (self.id, self.bytes)
    }
}

#[derive(Clone, PartialEq, Eq, SerdeInternal)]
pub enum LocalRequestOrResponseId {
    Request(LocalRequestId),
    Response(LocalResponseId),
}

impl LocalRequestOrResponseId {
    pub fn is_request(&self) -> bool {
        match self {
            LocalRequestOrResponseId::Request(_) => true,
            LocalRequestOrResponseId::Response(_) => false,
        }
    }

    pub fn is_response(&self) -> bool {
        match self {
            LocalRequestOrResponseId::Request(_) => false,
            LocalRequestOrResponseId::Response(_) => true,
        }
    }

    pub fn to_request_id(&self) -> LocalRequestId {
        match self {
            LocalRequestOrResponseId::Request(id) => *id,
            LocalRequestOrResponseId::Response(_) => {
                panic!("LocalRequestOrResponseId is a response")
            }
        }
    }

    pub fn to_response_id(&self) -> LocalResponseId {
        match self {
            LocalRequestOrResponseId::Request(_) => panic!("LocalRequestOrResponseId is a request"),
            LocalRequestOrResponseId::Response(id) => *id,
        }
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq, SerdeInternal)]
pub struct LocalRequestId {
    id: u8,
}

impl LocalRequestId {
    pub fn to_req_res_id(&self) -> LocalRequestOrResponseId {
        LocalRequestOrResponseId::Request(*self)
    }

    pub fn receive_from_remote(&self) -> LocalResponseId {
        LocalResponseId { id: self.id }
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

#[derive(Clone, Copy, Eq, Hash, PartialEq, SerdeInternal)]
pub struct LocalResponseId {
    id: u8,
}

impl LocalResponseId {
    pub fn to_req_res_id(&self) -> LocalRequestOrResponseId {
        LocalRequestOrResponseId::Response(*self)
    }

    pub fn receive_from_remote(&self) -> LocalRequestId {
        LocalRequestId { id: self.id }
    }
}
