use naia_socket_shared::IdentityToken;

pub enum IdentityReceiverResult {
    Waiting,
    Success(IdentityToken),
    /// The server refused the connection with an HTTP-style status code, and
    /// optionally an already-decoded message body explaining why
    /// (naia-lib/naia#133). The bytes are opaque here; the client crate decodes
    /// them against its protocol.
    ErrorResponseCode(u16, Option<Vec<u8>>),
}
