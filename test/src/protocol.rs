use naia_shared::Protocolize;

use super::auth::Auth;

#[derive(Protocolize)]
pub enum Protocol {
    Auth(Auth),
}
