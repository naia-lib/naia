use naia_shared::handshake::HandshakeHeader;

use crate::handshake::{HandshakeError, Handshaker};

pub struct HandshakeManager {

}

impl HandshakeManager {
    pub fn new() -> Self {

        //let header = HandshakeHeader::new(); // testing

        Self {

        }
    }
}

impl Handshaker for HandshakeManager {
    fn example(&self) -> Result<(), HandshakeError> {
        Ok(())
    }
}