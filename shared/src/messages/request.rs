use std::marker::PhantomData;

use crate::{Message, types::GlobalRequestId};

// Request
pub trait Request: Message {
    type Response: Response;
}

// Response
pub trait Response: Message {}

// ResponseSendKey
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct ResponseSendKey<S: Response> {
    request_id: GlobalRequestId,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseSendKey<S> {
    pub fn new(id: GlobalRequestId) -> Self {
        Self {
            request_id: id,
            phantom_s: PhantomData,
        }
    }

    pub fn request_id(&self) -> GlobalRequestId {
        self.request_id
    }
}

// ResponseReceiveKey
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct ResponseReceiveKey<S: Response> {
    response_id: GlobalRequestId,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseReceiveKey<S> {
    pub fn new(request_id: GlobalRequestId) -> Self {
        Self {
            response_id: request_id,
            phantom_s: PhantomData,
        }
    }

    pub fn response_id(&self) -> GlobalRequestId {
        self.response_id
    }
}