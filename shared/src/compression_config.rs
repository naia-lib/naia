
#[derive(Clone)]
pub struct CompressionConfig {
    pub server_to_client: Option<()>,
    pub client_to_server: Option<()>,
}

impl CompressionConfig {
    pub fn new(server_to_client: Option<()>, client_to_server: Option<()>) -> Self {
        Self {
            server_to_client,
            client_to_server,
        }
    }
}