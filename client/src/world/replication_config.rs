#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ReplicationConfig {
    Private,   // this is for Client non-Public Entities
    Public,    // this is for Server Entities and Client Public Entities
    Delegated, // this is for Server Delegated Entities
}

impl ReplicationConfig {
    pub fn is_delegated(&self) -> bool {
        match self {
            ReplicationConfig::Delegated => true,
            _ => false,
        }
    }
}
