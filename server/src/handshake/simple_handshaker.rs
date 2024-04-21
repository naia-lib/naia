use crate::handshake::{HandshakeError, Handshaker};

pub struct HandshakeManager {

}

impl HandshakeManager {
    pub fn new() -> Self {
        Self {

        }
    }
}

impl Handshaker for HandshakeManager {
    fn example(&self) -> Result<(), HandshakeError> {
        Ok(())
    }
}