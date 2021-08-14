use naia_shared::Ref;

use crate::{protocol::{KeyCommand, Point}};

const SQUARE_SPEED: u16 = 8;

pub fn process_command(key_command_ref: &Ref<KeyCommand>, point_ref: &Ref<Point>) {
    let key_command = key_command_ref.borrow();
    let old_x: u16;
    let old_y: u16;
    {
        let replicate_ref = point_ref.borrow();
        old_x = *(replicate_ref.x.get());
        old_y = *(replicate_ref.y.get());
    }
    if *key_command.w.get() {
        point_ref
            .borrow_mut()
            .y
            .set(old_y.wrapping_sub(SQUARE_SPEED))
    }
    if *key_command.s.get() {
        point_ref
            .borrow_mut()
            .y
            .set(old_y.wrapping_add(SQUARE_SPEED))
    }
    if *key_command.a.get() {
        point_ref
            .borrow_mut()
            .x
            .set(old_x.wrapping_sub(SQUARE_SPEED))
    }
    if *key_command.d.get() {
        point_ref
            .borrow_mut()
            .x
            .set(old_x.wrapping_add(SQUARE_SPEED))
    }
}
