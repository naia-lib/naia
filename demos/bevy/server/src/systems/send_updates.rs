use crate::aliases::Server;

pub fn send_updates(mut server: Server) {
    server.send_all_updates();
    server.tick_finish();
}
