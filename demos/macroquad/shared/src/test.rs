
#[derive_serde]
pub struct Auth {
    some_string: String,
    some_int: i32,
    some_bool: bool,
    some_tuple: (bool, i32, bool),
}