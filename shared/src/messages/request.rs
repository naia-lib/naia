use std::marker::PhantomData;

use crate::Message;

// Request
pub trait Request: Message {
    type Response: Response;
}

// Response
pub trait Response: Message {
}

// ResponseSendKey
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct ResponseSendKey<S: Response> {
    response_id: GlobalResponseId,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseSendKey<S> {
    pub fn new(id: GlobalResponseId) -> Self {
        Self {
            response_id: id,
            phantom_s: PhantomData,
        }
    }

    pub fn response_id(&self) -> GlobalResponseId {
        self.response_id
    }
}

// ResponseReceiveKey
#[derive(Clone, Eq, PartialEq, Hash, Copy)]
pub struct ResponseReceiveKey<S: Response> {
    request_id: GlobalRequestId,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseReceiveKey<S> {
    pub fn new(request_id: GlobalRequestId) -> Self {
        Self {
            request_id,
            phantom_s: PhantomData,
        }
    }

    pub fn request_id(&self) -> GlobalRequestId {
        self.request_id
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct GlobalRequestId {
    id: u64,
}

impl GlobalRequestId {
    pub fn new(id: u64) -> Self {
        Self {
            id,
        }
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct GlobalResponseId {
    id: u64,
}

impl GlobalResponseId {
    pub fn new(id: u64) -> Self {
        Self {
            id,
        }
    }
}