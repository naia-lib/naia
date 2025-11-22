use naia_client::transport::local::Socket as LocalClientSocket;
use naia_server::transport::local::Socket as LocalServerSocket;

use local_transport::LocalSocketPair;

pub fn local_socket_pair() -> (LocalClientSocket, LocalServerSocket) {
    let pair = LocalSocketPair::new();
    (
        LocalClientSocket::new(pair.client_socket, None),
        LocalServerSocket::new(pair.server_socket, None),
    )
}
