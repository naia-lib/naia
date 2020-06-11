use crate::{StateMask};

pub trait EntityType: Clone {
    fn read(&mut self, bytes: &[u8]);
    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8]);
    fn print(&self, key: u16);
}