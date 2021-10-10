use naia_bevy_demo_shared::protocol::Protocol;
use naia_bevy_server::Server;

pub fn send_updates(mut server: Server<Protocol>) {
    server.send_all_updates();
    server.tick_finish();
}
