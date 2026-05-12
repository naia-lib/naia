use std::marker::PhantomData;

use crate::Message;

/// Marker trait for message types that expect a typed response.
pub trait Request: Message {
    /// The corresponding response type returned by the remote endpoint.
    type Response: Response;
}

/// Marker trait for message types that are sent as a reply to a `Request`.
pub trait Response: Message {}

/// Typed token held by the sender to identify a pending request when its response arrives.
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct ResponseSendKey<S: Response> {
    response_id: GlobalResponseId,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseSendKey<S> {
    /// Creates a `ResponseSendKey` tied to the given global response ID.
    pub fn new(id: GlobalResponseId) -> Self {
        Self {
            response_id: id,
            phantom_s: PhantomData,
        }
    }

    /// Returns the global response ID carried by this key.
    pub fn response_id(&self) -> GlobalResponseId {
        self.response_id
    }
}

/// Typed token held by the receiver to identify which request a response answers.
#[derive(Clone, Eq, PartialEq, Hash, Copy)]
pub struct ResponseReceiveKey<S: Response> {
    request_id: GlobalRequestId,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseReceiveKey<S> {
    /// Creates a `ResponseReceiveKey` tied to the given global request ID.
    pub fn new(request_id: GlobalRequestId) -> Self {
        Self {
            request_id,
            phantom_s: PhantomData,
        }
    }

    /// Returns the global request ID carried by this key.
    pub fn request_id(&self) -> GlobalRequestId {
        self.request_id
    }
}

/// Globally-unique identifier for an outgoing request, spanning all connections.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GlobalRequestId {
    id: u64,
}

impl GlobalRequestId {
    /// Creates a `GlobalRequestId` from a raw u64.
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

/// Globally-unique identifier for a response to a specific request.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct GlobalResponseId {
    id: u64,
}

impl GlobalResponseId {
    /// Creates a `GlobalResponseId` from a raw u64.
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}
