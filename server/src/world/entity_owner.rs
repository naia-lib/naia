use crate::UserKey;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum EntityOwner {
    Server,
    Client(UserKey),
    ClientWaiting,
    Local,
}
