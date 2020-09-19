use std::{
    convert::From,
    ops::{Add, Sub},
};

/// Returns an interpolation from one value to another by a specified amount
pub fn interp_lerp<T: Sub + Add + Copy + Into<f32> + From<f32>>(
    old_value: &T,
    new_value: &T,
    fraction: f32,
) -> T {
    let old_float: f32 = T::into(*old_value);
    let new_float: f32 = T::into(*new_value);
    let output_f32 = ((old_float - new_float) * fraction) + new_float;
    let output: T = T::from(output_f32);
    output
}
