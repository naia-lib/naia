use naia_shared::{ChannelSender, EntityAuthEvent, GlobalEntity, ReliableSender};

const RESEND_EVENT_RTT_FACTOR: f32 = 1.5;

pub struct AuthorityManager {
    incoming: ReliableSender<EntityAuthEvent>,
}

impl AuthorityManager {
    pub fn new() -> Self {
        Self {
            incoming: ReliableSender::new(RESEND_EVENT_RTT_FACTOR),
        }
    }

    pub(crate) fn publish_entity(&mut self, global_entity: &GlobalEntity) {
        self.incoming
            .send_message(EntityAuthEvent::new_publish(&global_entity));
    }
}
