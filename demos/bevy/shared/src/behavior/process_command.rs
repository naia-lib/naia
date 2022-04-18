use crate::protocol::{KeyCommand, Position};

const SQUARE_SPEED: i16 = 3;

pub fn process_command(key_command: &KeyCommand, position: &mut Position) {
    if *key_command.w {
        *position.y = position.y.wrapping_sub(SQUARE_SPEED);
    }
    if *key_command.s {
        *position.y = position.y.wrapping_add(SQUARE_SPEED);
    }
    if *key_command.a {
        *position.x = position.x.wrapping_sub(SQUARE_SPEED);
    }
    if *key_command.d {
        *position.x = position.x.wrapping_add(SQUARE_SPEED);
    }
}
