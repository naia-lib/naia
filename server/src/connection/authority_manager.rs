use naia_shared::OrderedReliableReceiver;

pub struct AuthorityManager {
    incoming: OrderedReliableReceiver,
}

impl AuthorityManager {
    pub fn new() -> Self {
        Self {
            incoming: OrderedReliableReceiver::new(),
        }
    }
}
