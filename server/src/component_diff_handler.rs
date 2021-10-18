use naia_shared::PropertyMutate;

use super::{global_diff_handler::DiffSender, keys::ComponentKey};

#[derive(Clone)]
pub struct ComponentDiffHandler {
    component_key: ComponentKey,
    diff_sender: DiffSender,
}

impl ComponentDiffHandler {
    pub fn new(component_key: ComponentKey, diff_sender: DiffSender) -> Self {
        ComponentDiffHandler {
            component_key,
            diff_sender,
        }
    }
}

impl PropertyMutate for ComponentDiffHandler {
    fn mutate(&mut self, property_index: u8) {
        self.diff_sender.send((self.component_key, property_index));
    }
}
