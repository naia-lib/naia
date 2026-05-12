use std::{collections::HashMap, time::Duration};

use naia_derive::MessageRequest;
use naia_serde::{BitWriter, SerdeInternal};

use crate::messages::request::GlobalRequestId;
use crate::{KeyGenerator, LocalEntityAndGlobalEntityConverterMut, MessageContainer, MessageKinds};

/// Manages the lifecycle of outgoing requests and their local-to-global ID mapping.
pub struct RequestSender {
    local_key_generator: KeyGenerator<LocalRequestId>,
    local_to_global_ids: HashMap<LocalRequestId, GlobalRequestId>,
}

impl RequestSender {
    /// Creates a new `RequestSender` with a 60-second local-ID recycle window.
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
        MessageContainer::new(Box::new(request_message))
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
        MessageContainer::new(Box::new(response_message))
    }

    pub(crate) fn process_incoming_response(
        &mut self,
        local_request_id: &LocalRequestId,
    ) -> Option<GlobalRequestId> {
        self.local_key_generator.recycle_key(local_request_id);
        self.local_to_global_ids.remove(local_request_id)
    }
}

/// Wire envelope that carries either a request or a response payload with its local correlation ID.
#[derive(MessageRequest)]
pub struct RequestOrResponse {
    id: LocalRequestOrResponseId,
    bytes: Box<[u8]>,
}

impl RequestOrResponse {
    /// Wraps `bytes` as a request tagged with `id`.
    pub fn request(id: LocalRequestId, bytes: Box<[u8]>) -> Self {
        Self {
            id: id.to_req_res_id(),
            bytes,
        }
    }

    /// Wraps `bytes` as a response tagged with `id`.
    pub fn response(id: LocalResponseId, bytes: Box<[u8]>) -> Self {
        Self {
            id: id.to_req_res_id(),
            bytes,
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_id_and_bytes(self) -> (LocalRequestOrResponseId, Box<[u8]>) {
        (self.id, self.bytes)
    }
}

/// Connection-local discriminated ID that identifies a packet as carrying a request or a response.
#[derive(Clone, PartialEq, Eq, SerdeInternal)]
pub enum LocalRequestOrResponseId {
    /// Packet carries an outgoing request with this local ID.
    Request(LocalRequestId),
    /// Packet carries a response to the request with this local ID.
    Response(LocalResponseId),
}

impl LocalRequestOrResponseId {
    /// Returns `true` if this ID represents a request.
    pub fn is_request(&self) -> bool {
        match self {
            LocalRequestOrResponseId::Request(_) => true,
            LocalRequestOrResponseId::Response(_) => false,
        }
    }

    /// Returns `true` if this ID represents a response.
    pub fn is_response(&self) -> bool {
        match self {
            LocalRequestOrResponseId::Request(_) => false,
            LocalRequestOrResponseId::Response(_) => true,
        }
    }

    /// Returns the inner `LocalRequestId`. Panics if this is a response.
    pub fn to_request_id(&self) -> LocalRequestId {
        match self {
            LocalRequestOrResponseId::Request(id) => *id,
            LocalRequestOrResponseId::Response(_) => {
                panic!("LocalRequestOrResponseId is a response")
            }
        }
    }

    /// Returns the inner `LocalResponseId`. Panics if this is a request.
    pub fn to_response_id(&self) -> LocalResponseId {
        match self {
            LocalRequestOrResponseId::Request(_) => panic!("LocalRequestOrResponseId is a request"),
            LocalRequestOrResponseId::Response(id) => *id,
        }
    }
}

/// Connection-scoped u8 key correlating an outgoing request with its eventual response.
#[derive(Clone, Copy, Eq, Hash, PartialEq, SerdeInternal)]
pub struct LocalRequestId {
    id: u8,
}

impl LocalRequestId {
    /// Wraps `self` as a `LocalRequestOrResponseId::Request`.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_req_res_id(&self) -> LocalRequestOrResponseId {
        LocalRequestOrResponseId::Request(*self)
    }

    /// Returns the `LocalResponseId` that the remote will use when replying to this request.
    pub fn receive_from_remote(&self) -> LocalResponseId {
        LocalResponseId { id: self.id }
    }
}

impl From<u16> for LocalRequestId {
    fn from(id: u16) -> Self {
        Self { id: id as u8 }
    }
}

impl From<LocalRequestId> for u16 {
    fn from(val: LocalRequestId) -> Self {
        val.id as u16
    }
}

/// Connection-scoped u8 key correlating an incoming response with the original request.
#[derive(Clone, Copy, Eq, Hash, PartialEq, SerdeInternal)]
pub struct LocalResponseId {
    id: u8,
}

impl LocalResponseId {
    /// Wraps `self` as a `LocalRequestOrResponseId::Response`.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_req_res_id(&self) -> LocalRequestOrResponseId {
        LocalRequestOrResponseId::Response(*self)
    }

    /// Returns the `LocalRequestId` that the remote assigned to the request this response answers.
    pub fn receive_from_remote(&self) -> LocalRequestId {
        LocalRequestId { id: self.id }
    }
}
