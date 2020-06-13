use crate::{StateMask};

pub trait EntityType {
    fn read(&mut self, bytes: &[u8]);
    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8]);
    fn print(&self, key: u16);
    fn init(&self) -> Self;
    fn clone_inner_rc(&self) -> Self;
}