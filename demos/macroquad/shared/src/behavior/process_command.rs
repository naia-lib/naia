use crate::protocol::{KeyCommand, Square};

const SQUARE_SPEED: u16 = 3;

pub fn process_command(key_command: &KeyCommand, square: &mut Square) {
    if *key_command.w {
        *square.y = square.y.wrapping_sub(SQUARE_SPEED);
    }
    if *key_command.s {
        *square.y = square.y.wrapping_add(SQUARE_SPEED);
    }
    if *key_command.a {
        *square.x = square.x.wrapping_sub(SQUARE_SPEED);
    }
    if *key_command.d {
        *square.x = square.x.wrapping_add(SQUARE_SPEED);
    }
}
