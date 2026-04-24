// This fixture must NOT compile.
// EntityProperty inside #[replicate(immutable)] is forbidden by the derive macro.
use naia_shared::{EntityProperty, Replicate};

#[derive(Replicate)]
#[replicate(immutable)]
pub struct ForbiddenImmutableEntityProperty {
    pub target: EntityProperty,
}

fn main() {}
