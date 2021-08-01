use naia_shared::Ref;

use crate::{events::KeyCommand, state::Point};

const SQUARE_SPEED: u16 = 8;

pub fn process_command(key_command: &KeyCommand, point_ref: &Ref<Point>) {
    let old_x: u16;
    let old_y: u16;
    {
        let actor_ref = point_ref.borrow();
        old_x = *(actor_ref.x.get());
        old_y = *(actor_ref.y.get());
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
