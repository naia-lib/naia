use std::collections::HashMap;
use naia_shared::{NetEntity, Protocolize};

use super::locality_status::LocalityStatus;

pub struct LocalEntityRecord<P: Protocolize> {
    pub net_entity: NetEntity,
    pub status: LocalityStatus,
    pub components: HashMap<P::Kind, LocalityStatus>
}

impl<P: Protocolize> LocalEntityRecord<P> {
    pub fn new(net_entity: NetEntity) -> Self {
        LocalEntityRecord {
            net_entity,
            status: LocalityStatus::Creating,
            components: HashMap::new(),
        }
    }
}
