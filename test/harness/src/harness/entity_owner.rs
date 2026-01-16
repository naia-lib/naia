use crate::ClientKey;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EntityOwner {
    Server,
    Client(ClientKey),
    Local,
}