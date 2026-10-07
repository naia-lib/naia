use naia_socket_shared::IdentityToken;

/// Outcome of polling for the identity token the server sends over the
/// signaling channel during connection setup.
pub enum IdentityReceiverResult {
    /// No result has arrived yet.
    Waiting,
    /// The server accepted the connection and handed over this identity token.
    Success(IdentityToken),
    /// The server refused the connection with an HTTP-style status code, and
    /// optionally an already-decoded message body explaining why
    /// (naia-lib/naia#133). The bytes are opaque here; the client crate decodes
    /// them against its protocol.
    ErrorResponseCode(u16, Option<Vec<u8>>),
}
