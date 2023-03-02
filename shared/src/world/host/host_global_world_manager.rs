use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use crate::{GlobalDiffHandler, WorldRecord};

pub struct HostGlobalWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    pub diff_handler: Arc<RwLock<GlobalDiffHandler<E>>>,
    pub world_record: WorldRecord<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> HostGlobalWorldManager<E> {
    pub fn new() -> Self {
        Self {
            diff_handler: Arc::new(RwLock::new(GlobalDiffHandler::new())),
            world_record: WorldRecord::new(),
        }
    }
}
