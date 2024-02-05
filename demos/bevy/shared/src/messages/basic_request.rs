use naia_bevy_shared::{Message, Request, Response};

#[derive(Message)]
pub struct BasicRequest {
    pub contents: String,
}

impl Request for BasicRequest {
    type Response = BasicResponse;
}

impl BasicRequest {
    pub fn new(contents: String) -> Self {
        Self { contents }
    }
}

#[derive(Message)]
pub struct BasicResponse {
    pub contents: String,
}

impl Response for BasicResponse {}

impl BasicResponse {
    pub fn new(contents: String) -> Self {
        Self { contents }
    }
}