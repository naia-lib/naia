use naia_serde::{OwnedBitReader, Serde};

use super::protocolize::{Protocolize, ProtocolKindType};

pub struct ComponentUpdate<K: ProtocolKindType> {
    kind: K,
    buffer: OwnedBitReader
}