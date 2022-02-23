#[derive(Clone)]
pub struct CompressionConfig {
    pub server_to_client: Option<DirectionalCompressionConfig>,
    pub client_to_server: Option<DirectionalCompressionConfig>,
}

impl CompressionConfig {
    pub fn new(
        server_to_client: Option<DirectionalCompressionConfig>,
        client_to_server: Option<DirectionalCompressionConfig>,
    ) -> Self {
        Self {
            server_to_client,
            client_to_server,
        }
    }
}

#[derive(Copy, Clone)]
pub struct DirectionalCompressionConfig {
    pub mode: CompressionMode,
}

impl DirectionalCompressionConfig {
    pub fn new(mode: CompressionMode) -> Self {
        Self { mode }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum CompressionMode {
    /// Regular compression mode
    Regular,
    /// Dictionary training mode.
    /// The usize parameter here describes the desired size of the dictionary (in Kilobytes).
    /// Obviously, the bigger the dictionary the better theoretical compression.
    Training(usize),
}

impl CompressionMode {
    pub fn is_training(&self) -> bool {
        match self {
            CompressionMode::Regular => false,
            CompressionMode::Training(_) => true,
        }
    }
}
