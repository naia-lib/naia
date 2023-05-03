#[derive(Copy, Clone, PartialEq, Eq)]
pub enum EntityOwner {
    Server,
    Client,
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
            EntityOwner::Client => true,
            _ => false,
        }
    }
}
