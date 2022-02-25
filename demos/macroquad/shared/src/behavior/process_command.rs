use crate::protocol::{KeyCommand, Square};

const SQUARE_SPEED: u16 = 3;

pub fn process_command(key_command: &KeyCommand, square: &mut Square) {
    let old_x = *square.x;
    let old_y = *square.y;
    if *key_command.w {
        *square.y = old_y.wrapping_sub(SQUARE_SPEED);
    }
    if *key_command.s {
        *square.y = old_y.wrapping_add(SQUARE_SPEED);
    }
    if *key_command.a {
        *square.x = old_x.wrapping_sub(SQUARE_SPEED);
    }
    if *key_command.d {
        *square.x = old_x.wrapping_add(SQUARE_SPEED);
    }
}
