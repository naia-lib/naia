use crate::UserKey;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EntityOwner {
    Server,
    Client(UserKey),
    ClientWaiting(UserKey),
    ClientPublic(UserKey),
    Local,
}
