/// Per-direction zstd compression settings for a connection.
#[derive(Clone)]
pub struct CompressionConfig {
    /// Compression applied to packets flowing from server to client.
    pub server_to_client: Option<CompressionMode>,
    /// Compression applied to packets flowing from client to server.
    pub client_to_server: Option<CompressionMode>,
}

impl CompressionConfig {
    /// Creates a `CompressionConfig` with the given per-direction modes.
    pub fn new(
        server_to_client: Option<CompressionMode>,
        client_to_server: Option<CompressionMode>,
    ) -> Self {
        Self {
            server_to_client,
            client_to_server,
        }
    }
}

/// Selects the zstd compression strategy applied to a direction of traffic.
#[derive(Clone, Eq, PartialEq)]
pub enum CompressionMode {
    /// Compression mode using default zstd dictionary.
    /// 1st i32 parameter here is the compression level from -7 (fastest) to 22
    /// (smallest).
    Default(i32),
    /// Compression mode using custom dictionary.
    /// 1st i32 parameter here is the compression level from -7 (fastest) to 22
    /// (smallest). 2nd `Vec<u8>` parameter here is the dictionary itself.
    Dictionary(i32, Vec<u8>),
    /// Dictionary training mode.
    /// 1st usize parameter here describes the desired number of samples
    /// (packets) to train on. Obviously, the more samples trained on, the
    /// better theoretical compression.
    Training(usize),
}
