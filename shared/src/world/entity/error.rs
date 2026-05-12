use std::error::Error;

/// Error returned when an entity look-up fails during entity-to-global conversion.
#[derive(Debug)]
pub struct EntityDoesNotExistError;
impl Error for EntityDoesNotExistError {}
impl std::fmt::Display for EntityDoesNotExistError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "Error while attempting to look-up an Entity value for conversion: Entity was not found!")
    }
}
