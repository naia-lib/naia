// This fixture must NOT compile.
// Property<T> inside #[replicate(immutable)] is forbidden by the derive macro.
use naia_shared::{Property, Replicate};

#[derive(Replicate)]
#[replicate(immutable)]
pub struct ForbiddenImmutableProperty {
    pub value: Property<u32>,
}

fn main() {}
