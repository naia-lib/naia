use naia_serde::SerdeInternal;

#[derive(SerdeInternal, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityAuthStatus {
    // as far as we know, no authority over entity has been granted
    Available,
    // host has requested authority, but it has not yet been granted
    Requested,
    // host has been granted authority over entity
    Granted,
    // host has been denied authority over entity (another host has claimed it)
    Denied,
}

impl EntityAuthStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, EntityAuthStatus::Available)
    }

    pub fn is_requested(&self) -> bool {
        matches!(self, EntityAuthStatus::Requested)
    }

    pub fn is_granted(&self) -> bool {
        matches!(self, EntityAuthStatus::Granted)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, EntityAuthStatus::Denied)
    }

    pub fn can_request(&self) -> bool {
        match self {
            EntityAuthStatus::Available => true,
            EntityAuthStatus::Requested => false,
            EntityAuthStatus::Granted => false,
            EntityAuthStatus::Denied => false,
        }
    }

    pub fn can_release(&self) -> bool {
        match self {
            EntityAuthStatus::Available => false,
            EntityAuthStatus::Requested => true,
            EntityAuthStatus::Granted => true,
            EntityAuthStatus::Denied => false,
        }
    }

    pub fn can_mutate(&self) -> bool {
        match self {
            EntityAuthStatus::Available => false,
            EntityAuthStatus::Requested => true,
            EntityAuthStatus::Granted => true,
            EntityAuthStatus::Denied => false,
        }
    }

    pub fn can_read(&self) -> bool {
        match self {
            EntityAuthStatus::Available => true,
            EntityAuthStatus::Requested => false,
            EntityAuthStatus::Granted => false,
            EntityAuthStatus::Denied => true,
        }
    }

    pub fn can_write(&self) -> bool {
        match self {
            EntityAuthStatus::Available => false,
            EntityAuthStatus::Requested => false,
            EntityAuthStatus::Granted => true,
            EntityAuthStatus::Denied => false,
        }
    }
}
