use std::marker::PhantomData;

use crate::Message;

// Request
pub trait Request: Message {
    type Response: Response;
}

// Response
pub trait Response: Message {}

// ResponseSendKey
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct ResponseSendKey<S: Response> {
    request_id: u64,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseSendKey<S> {
    pub fn new(id: u64) -> Self {
        Self {
            request_id: id,
            phantom_s: PhantomData,
        }
    }

    pub fn request_id(&self) -> u64 {
        self.request_id
    }
}

// ResponseReceiveKey
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct ResponseReceiveKey<S: Response> {
    response_id: u64,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseReceiveKey<S> {
    pub fn new(request_id: u64) -> Self {
        Self {
            response_id: request_id,
            phantom_s: PhantomData,
        }
    }

    pub fn response_id(&self) -> u64 {
        self.response_id
    }
}