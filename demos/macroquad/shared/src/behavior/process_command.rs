use naia_shared::Ref;

use crate::protocol::{KeyCommand, Square};

const SQUARE_SPEED: u16 = 8;

pub fn process_command(key_command_ref: &Ref<KeyCommand>, square_ref: &Ref<Square>) {
    let key_command = key_command_ref.borrow();
    let mut square = square_ref.borrow_mut();
    let old_x = *(square.x.get());
    let old_y = *(square.y.get());
    if *key_command.w.get() {
        square.y.set(old_y.wrapping_sub(SQUARE_SPEED))
    }
    if *key_command.s.get() {
        square.y.set(old_y.wrapping_add(SQUARE_SPEED))
    }
    if *key_command.a.get() {
        square.x.set(old_x.wrapping_sub(SQUARE_SPEED))
    }
    if *key_command.d.get() {
        square.x.set(old_x.wrapping_add(SQUARE_SPEED))
    }
}
