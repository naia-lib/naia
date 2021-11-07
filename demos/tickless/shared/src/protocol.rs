use naia_derive::ProtocolType;

use crate::text::Text;

#[derive(ProtocolType)]
pub enum Protocol {
    Text(Text),
}
