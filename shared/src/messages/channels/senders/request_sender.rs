use std::{collections::HashMap, time::Duration};

use naia_derive::MessageRequest;
use naia_serde::{SerdeInternal, VecBitWriter};

use crate::messages::abandonment::ConnectionRequestNonce;
use crate::messages::request::GlobalRequestId;
use crate::{KeyGenerator, LocalEntityAndGlobalEntityConverterMut, MessageContainer, MessageKinds};

/// Manages the lifecycle of outgoing requests and their local-to-global ID mapping.
///
/// H3: every outstanding exchange is additionally keyed by its
/// [`ConnectionRequestNonce`]. A response resolves only when its wire
/// (local id, nonce) pair names the outstanding exchange; anything else —
/// stale duplicate, recycled-id alias, hostile packet — drops without
/// touching the mapping.
pub struct RequestSender {
    local_key_generator: KeyGenerator<LocalRequestId>,
    local_to_global_ids: HashMap<LocalRequestId, GlobalRequestId>,
    local_to_nonce: HashMap<LocalRequestId, ConnectionRequestNonce>,
}

impl RequestSender {
    /// Creates a new `RequestSender` with a 60-second local-ID recycle window.
    pub fn new() -> Self {
        Self {
            local_key_generator: KeyGenerator::new(Duration::from_mins(1)),
            local_to_global_ids: HashMap::new(),
            local_to_nonce: HashMap::new(),
        }
    }

    pub(crate) fn process_outgoing_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        global_request_id: GlobalRequestId,
        nonce: ConnectionRequestNonce,
        request: MessageContainer,
    ) -> MessageContainer {
        let local_request_id = self.local_key_generator.generate();
        self.local_to_global_ids
            .insert(local_request_id, global_request_id);
        self.local_to_nonce.insert(local_request_id, nonce);

        let mut writer = VecBitWriter::new();
        request.write(message_kinds, &mut writer, converter);
        let request_bytes = writer.to_bytes();
        let request_message = RequestOrResponse::request(local_request_id, nonce, request_bytes);
        MessageContainer::new(Box::new(request_message))
    }

    pub(crate) fn process_outgoing_response(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        local_response_id: LocalResponseId,
        nonce: ConnectionRequestNonce,
        response: MessageContainer,
    ) -> MessageContainer {
        let mut writer = VecBitWriter::new();
        response.write(message_kinds, &mut writer, converter);
        let response_bytes = writer.to_bytes();
        let response_message =
            RequestOrResponse::response(local_response_id, nonce, response_bytes);
        MessageContainer::new(Box::new(response_message))
    }

    pub(crate) fn process_incoming_response(
        &mut self,
        local_request_id: LocalRequestId,
        wire_nonce: ConnectionRequestNonce,
    ) -> Option<GlobalRequestId> {
        // Both halves must name the outstanding exchange. A foreign nonce
        // recycles nothing: the exchange stays live so the real response
        // still resolves, and the packet drops on the unknown-id path.
        match (
            self.local_to_global_ids.get(&local_request_id),
            self.local_to_nonce.get(&local_request_id),
        ) {
            (Some(global), Some(recorded)) if *recorded == wire_nonce => {
                let global = *global;
                self.local_key_generator.recycle_key(&local_request_id);
                self.local_to_global_ids.remove(&local_request_id);
                self.local_to_nonce.remove(&local_request_id);
                Some(global)
            }
            _ => None,
        }
    }
}

/// Wire envelope that carries either a request or a response payload with
/// its local correlation ID and its H3 [`ConnectionRequestNonce`].
#[derive(MessageRequest)]
pub struct RequestOrResponse {
    id: LocalRequestOrResponseId,
    nonce: ConnectionRequestNonce,
    bytes: Box<[u8]>,
}

impl RequestOrResponse {
    /// Wraps `bytes` as a request tagged with `id` and `nonce`.
    #[must_use]
    pub fn request(id: LocalRequestId, nonce: ConnectionRequestNonce, bytes: Box<[u8]>) -> Self {
        Self {
            id: id.to_req_res_id(),
            nonce,
            bytes,
        }
    }

    /// Wraps `bytes` as a response tagged with `id` and `nonce`.
    #[must_use]
    pub fn response(id: LocalResponseId, nonce: ConnectionRequestNonce, bytes: Box<[u8]>) -> Self {
        Self {
            id: id.to_req_res_id(),
            nonce,
            bytes,
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_id_and_bytes(
        self,
    ) -> (LocalRequestOrResponseId, ConnectionRequestNonce, Box<[u8]>) {
        (self.id, self.nonce, self.bytes)
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
    #[must_use]
    pub fn is_request(&self) -> bool {
        match self {
            LocalRequestOrResponseId::Request(_) => true,
            LocalRequestOrResponseId::Response(_) => false,
        }
    }

    /// Returns `true` if this ID represents a response.
    #[must_use]
    pub fn is_response(&self) -> bool {
        match self {
            LocalRequestOrResponseId::Request(_) => false,
            LocalRequestOrResponseId::Response(_) => true,
        }
    }

    /// Returns the inner `LocalRequestId`. Panics if this is a response.
    ///
    ///
    /// # Panics
    ///
    /// Panics when the invalid state is reached: `LocalRequestOrResponseId` is a response.
    /// # Panics
    ///
    /// Panics when the invalid state is reached: `LocalRequestOrResponseId` is a response.
    #[must_use]
    pub fn to_request_id(&self) -> LocalRequestId {
        match self {
            LocalRequestOrResponseId::Request(id) => *id,
            LocalRequestOrResponseId::Response(_) => {
                panic!("LocalRequestOrResponseId is a response")
            }
        }
    }

    /// Returns the inner `LocalResponseId`. Panics if this is a request.
    ///
    /// # Panics
    ///
    /// Panics when the invalid state is reached: `LocalRequestOrResponseId` is a request.
    #[must_use]
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
    #[must_use]
    pub fn to_req_res_id(&self) -> LocalRequestOrResponseId {
        LocalRequestOrResponseId::Request(*self)
    }

    /// Returns the `LocalResponseId` that the remote will use when replying to this request.
    #[must_use]
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
        u16::from(val.id)
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
    #[must_use]
    pub fn to_req_res_id(&self) -> LocalRequestOrResponseId {
        LocalRequestOrResponseId::Response(*self)
    }

    /// Returns the `LocalRequestId` that the remote assigned to the request this response answers.
    #[must_use]
    pub fn receive_from_remote(&self) -> LocalRequestId {
        LocalRequestId { id: self.id }
    }

    /// Builds a response id with an arbitrary raw byte, standing in for one a
    /// hostile peer picked. The wire form is a single byte, so every value here
    /// is reachable from the network.
    #[cfg(test)]
    pub(crate) fn from_raw(id: u8) -> Self {
        Self { id }
    }
}

#[cfg(test)]
mod request_sender_tests {
    //! H3 envelope cutover pins: the `ConnectionRequestNonce` rides the
    //! `RequestOrResponse` envelope in both directions, and the transport
    //! matches responses by (local id, nonce) — a wire nonce that does not
    //! name the outstanding exchange drops instead of resolving.

    use super::*;
    use crate::messages::abandonment::ConnectionRequestNonce;
    use crate::{
        messages::request::GlobalRequestId, FakeEntityConverter, Message, MessageContainer,
        MessageKinds,
    };

    #[derive(Message)]
    struct ProbeRequest {
        value: u8,
    }

    fn kinds() -> MessageKinds {
        let mut kinds = MessageKinds::new();
        kinds.add_message::<ProbeRequest>();
        kinds
    }

    fn probe_container() -> MessageContainer {
        MessageContainer::new(Box::new(ProbeRequest { value: 3 }))
    }

    fn envelope_of(container: MessageContainer) -> RequestOrResponse {
        *container
            .to_boxed_any()
            .downcast::<RequestOrResponse>()
            .expect("request sender wraps payloads in the envelope")
    }

    #[test]
    fn outgoing_request_envelope_carries_its_nonce() {
        let mut sender = RequestSender::new();
        let mut converter = FakeEntityConverter;
        let nonce = ConnectionRequestNonce::from_wire(7);

        let wrapped = sender.process_outgoing_request(
            &kinds(),
            &mut converter,
            GlobalRequestId::new(11),
            nonce,
            probe_container(),
        );
        let envelope = envelope_of(wrapped);
        let (id, out_nonce, bytes) = envelope.to_id_and_bytes();

        assert!(id.is_request());
        assert_eq!(out_nonce, nonce);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn incoming_response_resolves_only_on_id_and_nonce_match() {
        let mut sender = RequestSender::new();
        let mut converter = FakeEntityConverter;
        let global_id = GlobalRequestId::new(11);
        let nonce = ConnectionRequestNonce::from_wire(7);

        let wrapped = sender.process_outgoing_request(
            &kinds(),
            &mut converter,
            global_id,
            nonce,
            probe_container(),
        );
        let (local_id, _, _) = envelope_of(wrapped).to_id_and_bytes();
        let local_request_id = local_id.to_request_id();

        // Exact (id, nonce) match resolves to the global id.
        assert_eq!(
            sender.process_incoming_response(local_request_id, nonce),
            Some(global_id)
        );
    }

    #[test]
    fn incoming_response_with_foreign_nonce_drops() {
        let mut sender = RequestSender::new();
        let mut converter = FakeEntityConverter;
        let nonce = ConnectionRequestNonce::from_wire(7);

        let wrapped = sender.process_outgoing_request(
            &kinds(),
            &mut converter,
            GlobalRequestId::new(11),
            nonce,
            probe_container(),
        );
        let (local_id, _, _) = envelope_of(wrapped).to_id_and_bytes();
        let local_request_id = local_id.to_request_id();

        // Same local id, wrong nonce: a stale or hostile packet that must
        // not resolve the outstanding exchange.
        assert_eq!(
            sender
                .process_incoming_response(local_request_id, ConnectionRequestNonce::from_wire(8)),
            None
        );
        // The outstanding exchange survives the drop and still resolves.
        assert_eq!(
            sender.process_incoming_response(local_request_id, nonce),
            Some(GlobalRequestId::new(11))
        );
    }
}
