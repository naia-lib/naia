use naia_client::transport::local::Socket as LocalClientSocket;
use naia_server::transport::local::Socket as LocalServerSocket;

use local_transport::LocalTransportBuilder;

pub fn local_socket_pair() -> (LocalClientSocket, LocalServerSocket) {
    let builder = LocalTransportBuilder::new();
    let (server_endpoint, client_endpoint) = builder.single_connection();
    (
        LocalClientSocket::new(client_endpoint.into_socket(), None),
        LocalServerSocket::new(server_endpoint.into_socket(), None),
    )
}
