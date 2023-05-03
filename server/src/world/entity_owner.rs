use crate::UserKey;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EntityOwner {
    Server,
    Client(UserKey),
    ClientWaiting(UserKey),
    ClientPublic(UserKey),
    Local,
}

impl EntityOwner {
    pub fn is_server(&self) -> bool {
        match self {
            EntityOwner::Server => true,
            _ => false,
        }
    }

    pub fn is_client(&self) -> bool {
        match self {
            EntityOwner::Client(_)
            | EntityOwner::ClientPublic(_)
            | EntityOwner::ClientWaiting(_) => true,
            _ => false,
        }
    }
}
