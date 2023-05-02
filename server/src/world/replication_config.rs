#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ReplicationConfig {
    Private, // this is for Remote Entities
    Public,
    Dynamic,
}
