use naia_derive::Protocolize;

use crate::text::Text;

#[derive(Protocolize)]
pub enum Protocol {
    Text(Text),
}
