use crate::StateMask;

pub trait EntityType {
    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8]);
}
