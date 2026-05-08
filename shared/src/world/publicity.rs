/// Publication visibility of an entity.
///
/// Used by both server (`ReplicationConfig.publicity`) and client
/// (`configure_replication`) to express whether an entity replicates to
/// other peers and whether authority can be delegated to a client.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Publicity {
    /// Entity is owned by a client but not yet published to other peers.
    Private,
    /// Entity replicates to all peers within scope.
    Public,
    /// Server can delegate authority over this entity to a client.
    Delegated,
}

impl Publicity {
    pub fn is_delegated(&self) -> bool {
        matches!(self, Publicity::Delegated)
    }
}
