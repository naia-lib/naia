use naia_shared::Ref;

use crate::protocol::{KeyCommand, Position};

const SQUARE_SPEED: i16 = 8;

pub fn process_command(key_command_ref: &Ref<KeyCommand>, position_ref: &Ref<Position>) {
    let key_command = key_command_ref.borrow();
    let mut position = position_ref.borrow_mut();
    let old_x = *(position.x.get());
    let old_y = *(position.y.get());
    if *key_command.w.get() {
        position.y.set(old_y.wrapping_sub(SQUARE_SPEED))
    }
    if *key_command.s.get() {
        position.y.set(old_y.wrapping_add(SQUARE_SPEED))
    }
    if *key_command.a.get() {
        position.x.set(old_x.wrapping_sub(SQUARE_SPEED))
    }
    if *key_command.d.get() {
        position.x.set(old_x.wrapping_add(SQUARE_SPEED))
    }
}