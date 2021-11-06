use naia_derive::ProtocolType;

use crate::text::Text;

#[derive(ProtocolType, Clone)]
pub enum Protocol {
    Text(Text),
}
