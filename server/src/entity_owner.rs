use crate::UserKey;

pub enum EntityOwner {
    Server,
    Client(UserKey),
    Local,
}
