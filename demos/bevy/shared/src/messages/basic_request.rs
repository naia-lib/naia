use naia_bevy_shared::{Message, Request, Response};

#[derive(Message, Debug)]
pub struct BasicRequest {
    pub contents: String,
    pub index: u8,
}

impl Request for BasicRequest {
    type Response = BasicResponse;
}

impl BasicRequest {
    pub fn new(contents: String, index: u8) -> Self {
        Self { contents, index }
    }
}

#[derive(Message, Eq, PartialEq, Hash, Debug)]
pub struct BasicResponse {
    pub contents: String,
    pub index: u8,
}

impl Response for BasicResponse {}

impl BasicResponse {
    pub fn new(contents: String, index: u8) -> Self {
        Self { contents, index }
    }
}
