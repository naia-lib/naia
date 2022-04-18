use naia_shared::Protocolize;

use crate::text::Text;

#[derive(Protocolize)]
pub enum Protocol {
    Text(Text),
}
