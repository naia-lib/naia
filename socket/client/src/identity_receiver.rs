use naia_socket_shared::IdentityToken;

pub enum IdentityReceiverResult {
    Waiting,
    Success(IdentityToken),
    ErrorResponseCode(u16),
}
