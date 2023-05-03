#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityAuthStatus {
    // as far as we know, no authority over entity has been granted
    AvailableAuthority,
    // host has requested authority, but it has not yet been granted
    RequestedAuthority,
    // host has been granted authority over entity
    HasAuthority,
    // host has been denied authority over entity (another host has claimed it)
    NoAuthority,
}
