use naia_shared::PropertyMutate;

use super::mutcaster::MutSender;

#[derive(Clone)]
pub struct ComponentDiffHandler {
    mut_sender: MutSender,
}

impl ComponentDiffHandler {
    pub fn new(mut_sender: &MutSender) -> Self {
        ComponentDiffHandler {
            mut_sender: mut_sender.clone(),
        }
    }
}

impl PropertyMutate for ComponentDiffHandler {
    fn mutate(&mut self, property_index: u8) {
        self.mut_sender.send(property_index);
    }
}