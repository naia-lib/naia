use naia_socket_shared::IdentityToken;

/// The server application's answer to a pending client auth request.
///
/// This travels from the app thread back to the session task, which turns it
/// into the HTTP response the connecting client sees.
#[derive(Clone, Debug)]
pub enum AuthResponse {
    /// The request was accepted; the client is handed this identity token.
    Accept(IdentityToken),
    /// The request was refused, optionally carrying an already-serialized
    /// message explaining why (see naia-lib/naia#133). The bytes are opaque
    /// here -- the session task base64-encodes them into the 401 body, and the
    /// client decodes them against its own protocol.
    Reject(Option<Vec<u8>>),
}
