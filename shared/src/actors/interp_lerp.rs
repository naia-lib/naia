use std::ops::{Add, Sub};

pub trait InterpLerpable: Sized + Sub + Add + Clone + Copy + PartialEq {
    fn to_f32(&self) -> f32;
    fn from_f32(input: f32) -> Self;
}

/// Returns an interpolation from one value to another by a specified amount
pub fn interp_lerp<T: InterpLerpable>(old_value: &T, new_value: &T, fraction: f32) -> T {
    if fraction == 0.0 || PartialEq::eq(old_value, new_value) {
        return (*old_value).clone();
    }
    if fraction == 1.0 {
        return (*new_value).clone();
    }
    let old_float: f32 = old_value.to_f32();
    let new_float: f32 = new_value.to_f32();
    let output_f32 = ((new_float - old_float) * fraction) + old_float;
    let output: T = T::from_f32(output_f32);
    output
}

impl InterpLerpable for u16 {
    fn to_f32(&self) -> f32 {
        *self as f32
    }

    fn from_f32(input: f32) -> Self {
        input as Self
    }
}
