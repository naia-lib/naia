#[derive(Copy, Clone, PartialEq, Eq)]
pub enum EntityOwner {
    Server,
    Client,
    Local,
}
